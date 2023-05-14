use std::borrow::Cow;
use crate::error::{Error, Result};
use crate::idx::Idx;
use crate::ifo::Ifo;
use crate::dict::Dict;

use std::path::PathBuf;

pub struct StarDict {
	pub ifo: Ifo,
	idx: Idx,
	dict: Dict,
}

pub struct LookupResult<'a> {
	pub word: &'a str,
	pub trans: Cow<'a, str>,
}

impl StarDict {
	pub fn new(path: &PathBuf) -> Result<StarDict> {
		fn get_sub_file(prefix: &str, name: &'static str, compress_suffix: &'static str)
			-> Result<(PathBuf, bool)> {
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
			let idx = Idx::new(idx, &ifo, idx_gz, syn)?;
			let dict = Dict::new(dict, dict_bz)?;

			Ok(StarDict { ifo, idx, dict })
		} else {
			Err(Error::NoFileFound("ifo"))
		}
	}

	pub fn lookup<'a>(&'a mut self, word: &'a str) -> Option<LookupResult<'a>> {
		let entry = self.idx.lookup(word)?;
		let trans = self.dict.get_definition(entry)?;
		Some(LookupResult { word: &entry.word, trans })
	}

	pub fn dict_name(&self) -> &str {
		&self.ifo.bookname
	}

	pub fn wordcount(&self) -> usize {
		self.ifo.wordcount
	}
}
