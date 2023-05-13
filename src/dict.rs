use std::fs;
use std::io::Read;
use crate::error::{Error, Result};

use flate2::read::GzDecoder;
use std::path::PathBuf;
use crate::idx::IdxEntry;

pub struct Dict {
	contents: String,
}

impl<'a> Dict {
	pub fn new(path: PathBuf, bz: bool) -> Result<Dict> {
		let data = fs::read(path).map_err(|e| Error::FailedOpenFile("dict", e))?;
		let contents = if bz {
			let mut decoder = GzDecoder::new(data.as_slice());
			let mut contents = String::new();
			decoder.read_to_string(&mut contents)
				.map_err(|_| Error::InvalidDict)?;
			contents
		} else {
			String::from_utf8(data).map_err(|_| Error::InvalidDict)?
		};
		Ok(Dict { contents })
	}

	pub fn get_trans(&'a self, idx: &IdxEntry) -> Option<&'a str> {
		let offset = idx.offset;
		let size = idx.size;
		if offset + size > self.contents.len() {
			None
		} else {
			Some(&self.contents[offset..offset + size])
		}
	}
}
