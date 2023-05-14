use std::borrow::Cow;
use std::fs;
use std::fs::OpenOptions;
use std::io::BufReader;
use crate::error::{Error, Result};

use std::path::PathBuf;
use crate::dictzip::DictZip;
use crate::idx::IdxEntry;

enum DictInner {
	Plain(String),
	DictZip(DictZip),
}

pub struct Dict {
	inner: DictInner,
}

impl<'a> Dict {
	pub fn new(path: PathBuf, bz: bool) -> Result<Dict> {
		let inner = if bz {
			let file = OpenOptions::new()
				.read(true)
				.open(path)
				.map_err(|e| Error::FailedOpenFile("dict", e))?;
			let reader = BufReader::new(file);
			let dictzip = DictZip::new(reader)?;
			DictInner::DictZip(dictzip)
		} else {
			let data = fs::read(path).map_err(|e| Error::FailedOpenFile("dict", e))?;
			let string = String::from_utf8(data).map_err(|_| Error::InvalidDictContent)?;
			DictInner::Plain(string)
		};
		Ok(Dict { inner })
	}

	pub fn get_definition(&'a mut self, idx: &IdxEntry) -> Option<Cow<str>> {
		let offset = idx.offset;
		let size = idx.size;
		match &mut self.inner {
			DictInner::Plain(contents) =>
				if offset + size > contents.len() {
					None
				} else {
					Some(Cow::Borrowed(&contents[offset..offset + size]))
				}
			DictInner::DictZip(dz) => {
				dz.get_text(offset, size)
			}
		}
	}
}
