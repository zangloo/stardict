use crate::dict::Dict;
use crate::error::{Error, Result};
use crate::idx::Idx;
use crate::ifo::Ifo;

use std::fs::OpenOptions;
use std::io::Read;
use std::path::PathBuf;

pub struct StarDict {
	path: PathBuf,

	pub ifo: Ifo,
	idx: Idx,
	dict: Dict,
}

pub struct WordDefinitionSegment {
	pub types: String,
	pub text: String,
}

pub struct WordDefinition {
	pub word: String,
	pub segments: Vec<WordDefinitionSegment>,
}

impl StarDict {
	pub fn new(path: impl Into<PathBuf>) -> Result<StarDict> {
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
			let idx = Idx::new(idx, &ifo, idx_gz, syn)?;
			let dict = Dict::new(dict, dict_bz)?;

			Ok(StarDict {
				path,
				ifo,
				idx,
				dict,
			})
		} else {
			Err(Error::NoFileFound("ifo"))
		}
	}

	pub fn lookup(&mut self, word: &str) -> Option<Vec<WordDefinition>> {
		let blocks = self.idx.lookup_blocks(word)?;
		let mut definitions = vec![];
		for block in blocks {
			if let Some(result) = self.dict.get_definition(block, &self.ifo) {
				definitions.push(result);
			}
		}
		Some(definitions)
	}

	pub fn get_resource(&self, href: &str) -> Option<Vec<u8>> {
		let mut path_str = href;
		if let Some(ch) = path_str.chars().nth(0) {
			if ch == '/' {
				path_str = &path_str[1..];
			}
			if path_str.len() > 0 {
				let mut path = self.path.join("res");
				for sub in path_str.split("/") {
					path = path.join(sub);
				}
				if path.exists() {
					if let Ok(mut file) = OpenOptions::new().read(true).open(path) {
						let mut buf = vec![];
						if file.read_to_end(&mut buf).is_ok() {
							return Some(buf);
						}
					}
				}
			}
		}
		None
	}

	pub fn dict_name(&self) -> &str {
		&self.ifo.bookname
	}
}
