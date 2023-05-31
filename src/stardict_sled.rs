use std::collections::HashSet;
use std::path::PathBuf;
use sled::{Config, Db};
use crate::error::{Error, Result};
use crate::{get_cache_dir, Ifo, StarDict, WordDefinition, WordDefinitionSegment};
use crate::dict::Dict;
use crate::idx::Idx;

pub const IDX_SLED_SUFFIX: &str = "idx.sled";
pub const SYN_SLED_SUFFIX: &str = "syn.sled";

pub struct StarDictCachedSled {
	path: PathBuf,
	ifo: Ifo,
	idx: Db,
	syn: Option<Db>,
}

impl StarDictCachedSled {
	pub(crate) fn new(path: PathBuf, ifo: Ifo, idx: PathBuf, idx_gz: bool,
		syn: Option<PathBuf>, dict: PathBuf, dict_dz: bool, cache_name: &str) -> Result<Self>
	{
		let (idx_cache, syn_cache) = get_cache_dir(
			&path, cache_name, IDX_SLED_SUFFIX, Some(SYN_SLED_SUFFIX))?;

		let (idx, syn) = if !idx_cache.exists() {
			import_cache(&ifo, idx_cache, syn_cache, idx, idx_gz, syn, dict, dict_dz)?
		} else {
			let idx = open_db(idx_cache)?;
			let syn = if let Some(syn_cache) = syn_cache {
				Some(open_db(syn_cache)?)
			} else {
				None
			};
			(idx, syn)
		};

		Ok(StarDictCachedSled {
			path,
			ifo,
			idx,
			syn,
		})
	}
}

impl StarDict for StarDictCachedSled {
	#[inline]
	fn path(&self) -> &PathBuf {
		&self.path
	}

	#[inline]
	fn ifo(&self) -> &Ifo {
		&self.ifo
	}

	fn lookup(&mut self, word: &str) -> Result<Option<Vec<WordDefinition>>> {
		let lowercase_word = word.to_lowercase();
		let mut vec = vec![];
		let mut found = HashSet::new();
		if let Some(definition) = get_definition(&self.idx, &lowercase_word)? {
			found.insert(definition.word.clone());
			vec.push(definition);
		}
		if let Some(syn) = &self.syn {
			if let Some(alias) = get_strings(&syn, &lowercase_word)? {
				for key in alias {
					if let Some(definition) = get_definition(&self.idx, &key)? {
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

fn import_cache(ifo: &Ifo, idx_cache: PathBuf, syn_cache: Option<PathBuf>,
	idx: PathBuf, idx_gz: bool, syn: Option<PathBuf>, dict: PathBuf,
	dict_dz: bool) -> Result<(Db, Option<Db>)>
{
	let idx = Idx::new(idx, ifo, idx_gz, syn.clone())?;
	let mut dict = Dict::new(dict, dict_dz)?;

	let idx_db = sled::open(&idx_cache).map_err(sled_error_map)?;
	let syn_db = if let Some(syn_cache) = &syn_cache {
		Some(sled::open(syn_cache).map_err(sled_error_map)?)
	} else {
		None
	};

	for (word, entry) in &idx.items {
		let definition = if let Some(definition) = dict.get_definition(entry, ifo)? {
			definition
		} else {
			return Err(Error::InvalidIdxBlock(word.to_owned()));
		};
		let key = word.to_lowercase();
		let mut buf = vec![];
		buf.append(&mut definition.word.into_bytes());
		buf.push(0);
		for segment in definition.segments {
			buf.append(&mut segment.types.into_bytes());
			buf.push(0);
			buf.append(&mut segment.text.into_bytes());
			buf.push(0);
		}
		idx_db.insert(key.as_bytes(), buf.as_slice())
			.map_err(sled_error_map)?;
	}

	if let Some(syn_db) = &syn_db {
		for (key, aliases) in idx.syn.unwrap() {
			let mut buf = vec![];
			for alias in aliases {
				buf.append(&mut alias.to_lowercase().into_bytes());
				buf.push(0);
			}
			syn_db.insert(key.to_lowercase().as_bytes(), buf.as_slice())
				.map_err(sled_error_map)?;
		}
	}
	Ok((idx_db, syn_db))
}

#[inline]
fn open_db(path: PathBuf) -> Result<Db>
{
	Config::new()
		.path(path)
		.create_new(false)
		.open()
		.map_err(sled_error_map)
}

#[inline]
fn sled_error_map(error: sled::Error) -> Error
{
	Error::FailedOpenCache(error.to_string())
}

fn get_definition(db: &Db, lowercase_key: &str) -> Result<Option<WordDefinition>>
{
	let strings = get_strings(db, lowercase_key)?;
	if let Some(strings) = strings {
		let mut iter = strings.into_iter();
		let word = iter.next().unwrap();
		let mut entry = WordDefinition { word, segments: vec![] };
		while let Some(types) = iter.next() {
			let text = iter.next().unwrap();
			entry.segments.push(WordDefinitionSegment { types, text });
		}
		Ok(Some(entry))
	} else {
		Ok(None)
	}
}

#[inline]
fn get_strings(db: &Db, lowercase_key: &str) -> Result<Option<Vec<String>>>
{
	let bytes = if let Some(bytes) = db
		.get(lowercase_key.as_bytes())
		.map_err(sled_error_map)? {
		bytes
	} else {
		return Ok(None);
	};
	let buf = bytes.as_ref();
	let mut strings = vec![];
	let mut start = 0;
	while start < buf.len() {
		let mut end = start;
		while bytes[end] != 0 {
			end += 1;
		}
		let str = String::from_utf8_lossy(&buf[start..end]).to_string();
		strings.push(str);
		start = end + 1;
	}
	Ok(Some(strings))
}