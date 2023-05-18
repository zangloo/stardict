use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom};
use crate::error::{Error, Result};

use std::path::PathBuf;
use crate::{buf_to_string, WordDefinitionSegment};
use crate::dictzip::DictZip;
use crate::idx::IdxEntry;
use crate::ifo::Ifo;
use crate::stardict::WordDefinition;

enum DictInner {
	Plain(BufReader<File>, usize),
	DictZip(DictZip),
}

pub struct Dict {
	inner: DictInner,
}

impl<'a> Dict {
	pub fn new(path: PathBuf, bz: bool) -> Result<Dict> {
		let file = OpenOptions::new()
			.read(true)
			.open(path)
			.map_err(|e| Error::FailedOpenFile("dict", e))?;
		let inner = if bz {
			let reader = BufReader::new(file);
			let dictzip = DictZip::new(reader)?;
			DictInner::DictZip(dictzip)
		} else {
			let file_size = file.metadata()?.len() as usize;
			let reader = BufReader::new(file);
			DictInner::Plain(reader, file_size)
		};
		Ok(Dict { inner })
	}

	pub fn get_definition(&mut self, idx: &IdxEntry, ifo: &Ifo) -> Option<WordDefinition> {
		let mut segments = vec![];
		for block in &idx.blocks {
			let offset = block.offset;
			let size = block.size;
			let result = match &mut self.inner {
				DictInner::Plain(reader, file_size) =>
					if offset + size <= *file_size {
						reader.seek(SeekFrom::Start(offset as u64)).ok()?;
						let mut buf = vec![0; size];
						reader.read_exact(&mut buf).ok()?;
						parse_data(&buf, &ifo.sametypesequence)
					} else {
						None
					}
				DictInner::DictZip(dz) => {
					let (buf, offset) = dz.get_segment_data(offset, size)?;
					let data = &buf[offset..offset + size];
					parse_data(data, &ifo.sametypesequence)
				}
			};

			if let Some((types, text)) = result {
				segments.push(WordDefinitionSegment {
					types,
					text,
				});
			}
		}

		if segments.len() == 0 {
			None
		} else {
			Some(WordDefinition {
				word: idx.word.clone(),
				segments,
			})
		}
	}
}

pub fn parse_data(data: &[u8], types: &str) -> Option<(String, String)> {
	let (types, text) = if types.len() == 0 {
		if data.len() < 2 {
			return None;
		}
		let mut types = String::new();
		types.push(data[0] as char);
		let text = buf_to_string(&data[1..]);
		(types, text)
	} else {
		(types.to_owned(), buf_to_string(&data[..]))
	};
	Some((types, text))
}
