use std::collections::HashSet;
use std::path::PathBuf;
use rusqlite::{Connection, OpenFlags, params};
use crate::error::{Error, Result};
use crate::{get_cache_dir, Ifo, StarDict, WordDefinition, WordDefinitionSegment};
use crate::dict::Dict;
use crate::idx::Idx;

pub const IDX_SQLITE_SUFFIX: &str = "sqlite";

pub struct StarDictCachedSqlite {
	path: PathBuf,
	ifo: Ifo,
	db: Connection,
	has_syn: bool,
}

impl StarDictCachedSqlite {
	pub(crate) fn new(path: PathBuf, ifo: Ifo, idx: PathBuf, idx_gz: bool,
		syn: Option<PathBuf>, dict: PathBuf, dict_dz: bool, cache_name: &str)
		-> Result<Self>
	{
		let (idx_cache, _) = get_cache_dir(
			&path, cache_name, IDX_SQLITE_SUFFIX, None)?;

		let has_syn = syn.is_some();
		if !idx_cache.exists() {
			let idx = Idx::new(idx, &ifo, idx_gz, syn.clone())?;
			let dict = Dict::new(dict, dict_dz)?;

			import_cache(&ifo, &idx_cache, idx, dict).map_err(sqlite_error_map)?;
		}
		let db = Connection::open_with_flags(idx_cache, OpenFlags::SQLITE_OPEN_READ_ONLY)
			.map_err(sqlite_error_map)?;

		Ok(StarDictCachedSqlite {
			path,
			ifo,
			db,
			has_syn,
		})
	}

	fn lookup_db(&self, lowercase_word: &str) -> core::result::Result<Option<Vec<WordDefinition>>, rusqlite::Error>
	{
		let mut vec = vec![];
		let mut found = HashSet::new();
		if let Some(definition) = self.query_definition(&lowercase_word)? {
			found.insert(definition.word.clone());
			vec.push(definition);
		}

		// now query aliases
		if self.has_syn {
			let mut stmt = self.db.prepare("select aliases from alias where word = ?")?;
			let mut rows = stmt.query([&lowercase_word])?;
			if let Some(row) = rows.next()? {
				let aliases: String = row.get(0)?;
				let aliases: Vec<String> = serde_json::from_str(&aliases).unwrap();

				for key in aliases {
					if let Some(definition) = self.query_definition(&key)? {
						if !found.contains(&definition.word) {
							found.insert(definition.word.clone());
							vec.push(definition);
						}
					}
				}
			}
		}

		let definitions = if vec.len() == 0 {
			None
		} else {
			Some(vec)
		};
		Ok(definitions)
	}

	fn query_definition(&self, lowercase_word: &str) -> core::result::Result<Option<WordDefinition>, rusqlite::Error>
	{
		let mut stmt = self.db.prepare("select id, definition from word where word in (?) order by id")?;
		let mut rows = stmt.query([lowercase_word])?;
		let (word_id, mut definition) = if let Some(row) = rows.next()? {
			let word_id: i64 = row.get(0)?;
			let word = row.get(1)?;
			let definition = WordDefinition { word, segments: vec![] };
			(word_id, definition)
		} else {
			return Ok(None);
		};
		drop(rows);
		stmt.finalize()?;

		stmt = self.db.prepare("select types, text from segment where word_id = ?")?;
		let mut rows = stmt.query([word_id])?;
		while let Some(row) = rows.next()? {
			let types = row.get(0)?;
			let text = row.get(1)?;
			definition.segments.push(WordDefinitionSegment { types, text });
		}
		drop(rows);
		stmt.finalize()?;
		Ok(Some(definition))
	}
}

impl StarDict for StarDictCachedSqlite {
	#[inline]
	fn path(&self) -> &PathBuf
	{
		&self.path
	}

	#[inline]
	fn ifo(&self) -> &Ifo
	{
		&self.ifo
	}

	#[inline]
	fn lookup(&mut self, word: &str) -> Result<Option<Vec<WordDefinition>>>
	{
		Ok(self.lookup_db(&word.to_lowercase()).map_err(sqlite_error_map)?)
	}
}

fn import_cache(ifo: &Ifo, idx_cache: &PathBuf, idx: Idx, mut dict: Dict)
	-> core::result::Result<Connection, rusqlite::Error>
{
	let db = Connection::open(&idx_cache)?;
	db.execute_batch(
		"create table word(id integer primary key, word text, definition text);
			create index word_idx on word(word);
			create table segment(id integer primary key, word_id integer, types text, text text);
			create index segment_idx on segment(word_id);
			create table alias(id integer primary key, word text, aliases text);
			create index alias_idx on alias(word);
			begin;")?;
	let mut definition_stmt = db.prepare("insert into word (word, definition) values (?, ?)")?;
	let mut segment_stmt = db.prepare("insert into segment (word_id, types, text) values (?, ?, ?)")?;
	for (word, entry) in &idx.items {
		let definition = if let Ok(Some(definition)) = dict.get_definition(entry, ifo) {
			definition
		} else {
			continue;
		};
		let key = word.to_lowercase();
		let word_id = definition_stmt.insert([&key, &definition.word])?;
		for segment in definition.segments {
			segment_stmt.execute(params![word_id, segment.types, segment.text])?;
		}
	}
	definition_stmt.finalize()?;
	segment_stmt.finalize()?;

	if let Some(syn) = &idx.syn {
		let mut alias_stmt = db.prepare("insert into alias (word, aliases) values (?, ?)")?;
		for (key, aliases) in syn {
			let aliases_json = serde_json::to_string(aliases).unwrap();
			alias_stmt.execute([&key.to_lowercase(), &aliases_json])?;
		}
		alias_stmt.finalize()?;
	}
	db.execute("commit", ())?;
	Ok(db)
}

#[inline]
fn sqlite_error_map(error: rusqlite::Error) -> Error
{
	Error::FailedOpenCache(error.to_string())
}
