pub mod error;
mod stardict;
mod idx;
mod ifo;
mod dict;
mod dictzip;
#[cfg(feature = "sled")]
mod stardict_sled;
#[cfg(feature = "sqlite")]
mod stardict_sqlite;

use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::PathBuf;
use dirs::cache_dir;
#[cfg(feature = "sqlite")]
use serde::{Serialize, Deserialize};

use crate::error::{Error, Result};
pub use crate::ifo::Ifo;
pub use crate::stardict::StarDictStd;
#[cfg(feature = "sled")]
pub use crate::stardict_sled::StarDictCachedSled;
#[cfg(feature = "sqlite")]
pub use crate::stardict_sqlite::StarDictCachedSqlite;

#[inline]
fn buf_to_string(buf: &[u8]) -> String {
	String::from_utf8_lossy(buf)
		.chars()
		.filter(|&c| c != '\u{fffd}')
		.collect()
}

#[derive(Debug)]
#[cfg_attr(feature = "sqlite", derive(Serialize, Deserialize))]
pub struct WordDefinitionSegment {
	pub types: String,
	pub text: String,
}

#[derive(Debug)]
#[cfg_attr(feature = "sqlite", derive(Serialize, Deserialize))]
pub struct WordDefinition {
	pub word: String,
	pub segments: Vec<WordDefinitionSegment>,
}

pub trait StarDict {
	fn path(&self) -> &PathBuf;
	fn ifo(&self) -> &Ifo;
	fn dict_name(&self) -> &str {
		&self.ifo().bookname
	}
	fn lookup(&mut self, word: &str) -> Result<Option<Vec<WordDefinition>>>;
	fn get_resource(&self, href: &str) -> Result<Option<Vec<u8>>> {
		let mut path_str = href;
		if let Some(ch) = path_str.chars().nth(0) {
			if ch == '/' {
				path_str = &path_str[1..];
			}
			if path_str.len() > 0 {
				let mut path = self.path().join("res");
				for sub in path_str.split("/") {
					path = path.join(sub);
				}
				if path.exists() {
					let mut file = OpenOptions::new()
						.read(true)
						.open(path)
						.map_err(|e| Error::FailedLoadResource(href.to_owned(), e.to_string()))?;
					let mut buf = vec![];
					file.read_to_end(&mut buf)
						.map_err(|e| Error::FailedLoadResource(href.to_owned(), e.to_string()))?;
					return Ok(Some(buf));
				}
			}
		}
		Err(Error::NoResourceFound(href.to_owned()))
	}
}

fn get_cache_dir<'a>(path: &'a PathBuf, cache_name: &str,
	idx_cache_suffix: &str, syn_cache_suffix: Option<&str>)
	-> Result<(PathBuf, Option<PathBuf>)>
{
	let dict_name = path.file_name().unwrap().to_str().unwrap();
	let cache_dir = cache_dir().ok_or_else(|| Error::NoCacheDir)?;
	let cache_dir = cache_dir.join(cache_name);
	if !cache_dir.exists() {
		fs::create_dir_all(&cache_dir)?;
	}
	let idx_cache_str = format!("{}.{}", dict_name, idx_cache_suffix);
	let idx_cache = cache_dir.join(&idx_cache_str);
	let syn_cache = if let Some(suffix) = syn_cache_suffix {
		let syn_cache_str = format!("{}.{}", dict_name, suffix);
		Some(cache_dir.join(&syn_cache_str))
	} else {
		None
	};

	Ok((idx_cache, syn_cache))
}

#[inline]
#[cfg(feature = "sled")]
pub fn with_sled(path: impl Into<PathBuf>, cache_name: &str)
	-> Result<StarDictCachedSled> {
	create(path, |path, ifo, idx, idx_gz, syn, dict, dict_bz|
		StarDictCachedSled::new(path, ifo, idx, idx_gz, syn, dict, dict_bz, cache_name))
}

#[inline]
#[cfg(feature = "sqlite")]
pub fn with_sqlite(path: impl Into<PathBuf>, cache_name: &str)
	-> Result<StarDictCachedSqlite> {
	create(path, |path, ifo, idx, idx_gz, syn, dict, dict_bz|
		StarDictCachedSqlite::new(path, ifo, idx, idx_gz, syn, dict, dict_bz, cache_name))
}

#[inline]
pub fn no_cache(path: impl Into<PathBuf>) -> Result<StarDictStd> {
	create(path, StarDictStd::new)
}

fn create<C, T>(path: impl Into<PathBuf>, creator: C) -> Result<T>
	where C: FnOnce(PathBuf, Ifo, PathBuf, bool, Option<PathBuf>, PathBuf, bool) -> Result<T>
{
	fn get_sub_file(
		prefix: &str,
		name: &'static str,
		compress_suffix: &'static str,
	) -> Result<(PathBuf, bool)> {
		let mut path_str = format!("{}.{}", prefix, name);
		let mut path = PathBuf::from(&path_str);
		if path.exists() {
			Ok((path, false))
		} else {
			path_str.push('.');
			path_str.push_str(compress_suffix);
			path = PathBuf::from(path_str);
			if path.exists() {
				Ok((path, true))
			} else {
				Err(Error::NoFileFound(name))
			}
		}
	}

	let mut ifo = None;
	let path = path.into();
	for p in path.read_dir().map_err(|e| Error::FailedOpenIfo(e))? {
		let path = p.map_err(|e| Error::FailedOpenIfo(e))?.path();
		if let Some(extension) = path.extension() {
			if extension.to_str().unwrap() == "ifo" {
				ifo = Some(path);
				break;
			}
		}
	}

	if let Some(ifo) = ifo {
		let ifo_path = ifo.to_str().unwrap();
		let prefix = &ifo_path[0..ifo_path.len() - 4];
		let (idx, idx_gz) = get_sub_file(prefix, "idx", "gz")?;
		let (dict, dict_bz) = get_sub_file(prefix, "dict", "dz")?;
		// optional syn file
		let syn_path = PathBuf::from(&format!("{}.syn", prefix));
		let syn = if syn_path.exists() {
			Some(syn_path)
		} else {
			None
		};

		let ifo = Ifo::new(ifo)?;
		creator(path, ifo, idx, idx_gz, syn, dict, dict_bz)
	} else {
		Err(Error::NoFileFound("ifo"))
	}
}

#[cfg(test)]
mod tests {
	use crate::StarDict;
	#[cfg(feature = "sled")]
	use crate::with_sled;
	#[cfg(feature = "sqlite")]
	use crate::with_sqlite;
	use crate::no_cache;

	const CACHE_NAME: &str = "test";
	const DICT: &str = "/home/zl/.stardict/dic/stardict-chibigenc-2.4.2/";
	const WORD: &str = "汉";
	const WORD_DEFINITION: &str = "漢";

	#[test]
	fn lookup() {
		let mut dict = no_cache(DICT).unwrap();
		let definitions = dict.lookup(WORD).unwrap().unwrap();
		assert_eq!(definitions.len(), 1);
		assert_eq!(definitions[0].word, WORD_DEFINITION);
		assert_eq!(definitions[0].segments.len(), 1);
		assert_eq!(definitions[0].segments[0].types, "g");
	}

	#[test]
	#[cfg(feature = "sled")]
	fn lookup_sled() {
		let mut dict = with_sled(DICT, CACHE_NAME).unwrap();
		let definitions = dict.lookup(WORD).unwrap().unwrap();
		assert_eq!(definitions.len(), 1);
		assert_eq!(definitions[0].word, WORD_DEFINITION);
		assert_eq!(definitions[0].segments.len(), 1);
		assert_eq!(definitions[0].segments[0].types, "g");

		let mut dict = no_cache(DICT).unwrap();
		let std_definitions = dict.lookup(WORD).unwrap().unwrap();
		for i in 0..definitions.len() {
			let cached = &definitions[i];
			let std = &std_definitions[i];
			assert_eq!(cached.word, std.word);
			for j in 0..cached.segments.len() {
				let c = &cached.segments[j];
				let s = &std.segments[j];
				assert_eq!(c.types, s.types);
				assert_eq!(c.text, s.text);
			}
		}
	}

	#[test]
	#[cfg(feature = "sqlite")]
	fn lookup_sqlite() {
		let mut dict = with_sqlite(DICT, CACHE_NAME).unwrap();
		let definitions = dict.lookup(WORD).unwrap().unwrap();
		assert_eq!(definitions.len(), 1);
		assert_eq!(definitions[0].word, WORD_DEFINITION);
		assert_eq!(definitions[0].segments.len(), 1);
		assert_eq!(definitions[0].segments[0].types, "g");

		let mut dict = no_cache(DICT).unwrap();
		let std_definitions = dict.lookup(WORD).unwrap().unwrap();
		for i in 0..definitions.len() {
			let cached = &definitions[i];
			let std = &std_definitions[i];
			assert_eq!(cached.word, std.word);
			for j in 0..cached.segments.len() {
				let c = &cached.segments[j];
				let s = &std.segments[j];
				assert_eq!(c.types, s.types);
				assert_eq!(c.text, s.text);
			}
		}
	}
}
