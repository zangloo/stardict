use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{fs, process, thread};
use std::str::FromStr;
use process_alive::{Pid, State};
use rusqlite::{Connection, OpenFlags, params};
use crate::error::{Error, Result};
use crate::{get_cache_dir, Ifo, StarDict, WordDefinition, WordDefinitionSegment};
use crate::dict::Dict;
use crate::idx::Idx;

pub const IDX_SQLITE_SUFFIX: &str = "sqlite";

enum InnerDb {
	Loaded(Connection),
	InitByOther(PathBuf, Connection),
	Init(PathBuf, Arc<Mutex<Connection>>),
}

pub struct StarDictCachedSqlite {
	path: PathBuf,
	ifo: Ifo,
	db: InnerDb,
	has_syn: bool,
}

impl StarDictCachedSqlite {
	pub(crate) fn new(path: PathBuf, ifo: Ifo, idx: PathBuf, idx_gz: bool,
		syn: Option<PathBuf>, dict: PathBuf, dict_dz: bool, cache_name: &str)
		-> Result<Self>
	{
		fn load_db(idx_cache: &PathBuf) -> Result<Option<InnerDb>>
		{
			if !idx_cache.exists() {
				return Ok(None);
			}
			let db = Connection::open_with_flags(idx_cache, OpenFlags::SQLITE_OPEN_READ_ONLY)
				.map_err(sqlite_error_map)?;
			if check_init_complete(&db).map_err(sqlite_error_map)? {
				return Ok(Some(InnerDb::Loaded(db)));
			}

			// another process is doing init now
			if other_pid_alive(&db, &idx_cache)? {
				return Ok(Some(InnerDb::InitByOther(idx_cache.clone(), db)));
			}

			// preview process end without init finished
			// remove it and do init again
			if let Err((_, err)) = db.close() {
				return Err(sqlite_error_map(err));
			}
			fs::remove_file(idx_cache)?;
			Ok(None)
		}

		let (idx_cache, _) = get_cache_dir(
			&path, cache_name, IDX_SQLITE_SUFFIX, None)?;

		let has_syn = syn.is_some();
		let inner = load_db(&idx_cache)?;

		let inner = if let Some(inner) = inner {
			inner
		} else {
			let db = Connection::open(&idx_cache).map_err(sqlite_error_map)?;
			init_db(&db)?;
			let idx = Idx::new(idx, &ifo, idx_gz, syn.clone())?;
			let dict = Dict::new(dict, dict_dz)?;

			let db = Arc::new(Mutex::new(db));
			let arc_db = db.clone();
			let idx_cache2 = idx_cache.clone();
			let ifo2 = ifo.clone();
			thread::spawn(move || {
				if let Ok(db) = arc_db.lock() {
					if let Err(_) = import_cache(&db, &ifo2, idx, dict) {
						eprint!("Failed import dictionary cache:{:#?}", idx_cache2);
					}
				};
			});

			InnerDb::Init(idx_cache, db.clone())
		};

		Ok(StarDictCachedSqlite {
			path,
			ifo,
			db: inner,
			has_syn,
		})
	}

	fn lookup_db(&self, db: &Connection, lowercase_word: &str) -> core::result::Result<Option<Vec<WordDefinition>>, rusqlite::Error>
	{
		let mut vec = vec![];
		let mut found = HashSet::new();
		if let Some(definition) = query_definition(db, &lowercase_word)? {
			found.insert(definition.word.clone());
			vec.push(definition);
		}

		// now query aliases
		if self.has_syn {
			let mut stmt = db.prepare("select aliases from alias where word = ?")?;
			let mut rows = stmt.query([&lowercase_word])?;
			if let Some(row) = rows.next()? {
				let aliases: String = row.get(0)?;
				let aliases: Vec<String> = serde_json::from_str(&aliases).unwrap();

				for key in aliases {
					if let Some(definition) = query_definition(db, &key)? {
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
		let reset_init = match &self.db {
			InnerDb::Loaded(_) => None,
			InnerDb::InitByOther(idx_cache, db) =>
				if Ok(true) == check_init_complete(db) {
					Some(idx_cache.clone())
				} else {
					return Err(Error::CacheInitiating);
				}
			InnerDb::Init(idx_cache, db) => {
				if let Ok(db) = db.try_lock() {
					match check_init_complete(&db) {
						Ok(true) => Some(idx_cache.clone()),
						_ => return Err(Error::CacheInitiating),
					}
				} else {
					// Initiating by current process
					return Err(Error::CacheInitiating);
				}
			}
		};
		if let Some(idx_cache) = reset_init {
			let db = Connection::open_with_flags(
				&idx_cache,
				OpenFlags::SQLITE_OPEN_READ_ONLY)
				.map_err(sqlite_error_map)?;
			self.db = InnerDb::Loaded(db);
		}
		if let InnerDb::Loaded(db) = &self.db {
			Ok(self.lookup_db(db, &word.to_lowercase()).map_err(sqlite_error_map)?)
		} else {
			panic!("noway")
		}
	}
}

fn init_db(db: &Connection) -> Result<()>
{
	let pid = process::id();
	db.execute_batch(
		"create table meta(key text, value text);
			create table word(id integer primary key, word text, definition text);
			create index word_idx on word(word);
			create table segment(id integer primary key, word_id integer, types text, text text);
			create index segment_idx on segment(word_id);
			create table alias(id integer primary key, word text, aliases text);
			create index alias_idx on alias(word);
			insert into meta(key, value) values ('version', '1');
			insert into meta(key, value) values ('init_status', 'start');")
		.map_err(sqlite_error_map)?;
	db.execute("insert into meta(key, value) values ('init_pid', ?)", [pid])
		.map_err(sqlite_error_map)?;
	Ok(())
}

fn import_cache(db: &Connection, ifo: &Ifo, idx: Idx, mut dict: Dict)
	-> core::result::Result<(), rusqlite::Error>
{
	db.execute("begin", ())?;
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
	db.execute("update meta set value = 'success' where key = 'init_status'", ())?;
	db.execute("commit", ())?;
	Ok(())
}

fn query_definition(db: &Connection, lowercase_word: &str) -> core::result::Result<Option<WordDefinition>, rusqlite::Error>
{
	let mut stmt = db.prepare("select id, definition from word where word in (?) order by id")?;
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

	stmt = db.prepare("select types, text from segment where word_id = ?")?;
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

#[inline]
fn check_init_complete(db: &Connection) -> core::result::Result<bool, rusqlite::Error>
{
	db.query_row("select value from meta where key = 'init_status'", (), |row| {
		let init_status: String = row.get(0)?;
		Ok(init_status == "success")
	})
}

#[inline]
fn sqlite_error_map(error: rusqlite::Error) -> Error
{
	Error::FailedOpenCache(error.to_string())
}

fn other_pid_alive(db: &Connection, idx_cache: &PathBuf) -> Result<bool>
{
	let init_pid = db.query_row("select value from meta where key = 'init_pid'", [], |row| {
		let init_pid: String = row.get(0)?;
		Ok(init_pid)
	}).map_err(sqlite_error_map)?;
	let init_pid = u32::from_str(&init_pid)
		.map_err(|_| Error::InvalidDictCache(format!("{:#?}", idx_cache)))?;
	let pid = Pid::from(init_pid);
	let state = process_alive::state(pid);

	// another process is doing init now
	Ok(matches!(state, State::Alive))
}