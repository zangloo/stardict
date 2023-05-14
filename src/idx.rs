use crate::error::{Error, Result};
use crate::ifo::{Ifo, Version};

use std::fmt::Debug;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use flate2::read::GzDecoder;
use std::collections::HashMap;
use byteorder::{BigEndian, ReadBytesExt};
use crate::buf_to_string;

#[derive(Debug)]
pub struct IdxEntry {
	pub word: String,
	pub offset: usize,
	pub size: usize,
}

#[derive(Debug)]
pub struct Idx {
	items: HashMap<String, IdxEntry>,
}

#[allow(unused)]
impl Idx {
	pub fn new(path: PathBuf, ifo: &Ifo, gz: bool, syn: Option<PathBuf>) -> Result<Idx> {
		let f = File::open(path).map_err(|e| Error::FailedOpenFile("idx", e))?;
		let mut reader = BufReader::new(f);
		let mut idx = if gz {
			let mut decoder = GzDecoder::new(reader);
			let mut buf = vec![];
			decoder.read_to_end(&mut buf);
			read(&ifo.version, ifo.idxoffsetbits, buf.as_slice(), syn)
		} else {
			read(&ifo.version, ifo.idxoffsetbits, reader, syn)
		}?;

		Ok(idx)
	}

	pub fn lookup(&self, word: &str) -> Option<&IdxEntry> {
		self.items.get(&word.to_lowercase())
	}
}

#[inline]
fn read(version: &Version, idxoffsetbits: usize, reader: impl BufRead, syn: Option<PathBuf>) -> Result<Idx> {
	let vec = match version {
		Version::V242 => read_items(reader, |r| Ok(r.read_u32::<BigEndian>()? as usize))?,
		Version::V300 => if idxoffsetbits == 64 {
			read_items(reader, |r| Ok(r.read_u64::<BigEndian>()? as usize))?
		} else {
			read_items(reader, |r| Ok(r.read_u32::<BigEndian>()? as usize))?
		}
	};
	let mut items = HashMap::new();
	if let Some(syn) = syn {
		load_syn(&vec, &mut items, syn)?;
	}
	vec.into_iter().for_each(|item| {
		items.insert(item.word.to_lowercase(), item);
	});
	Ok(Idx { items })
}

fn read_items<F>(mut reader: impl BufRead, f: F) -> Result<Vec<IdxEntry>>
	where F: Fn(&mut dyn BufRead) -> std::io::Result<usize>
{
	let mut items = vec![];
	let mut buf: Vec<u8> = Vec::new();
	loop {
		buf.clear();
		let read_bytes = reader.read_until(0, &mut buf)
			.map_err(|e| Error::FailedOpenFile("idx", e))?;
		if read_bytes == 0 {
			break;
		}

		if let Some(b'\0') = buf.last() {
			buf.pop();
		}

		let word = buf_to_string(&buf);
		let offset: usize = f(&mut reader).map_err(|_| Error::InvalidIdxElement("offset"))?;
		let size: usize = f(&mut reader).map_err(|_| Error::InvalidIdxElement("size"))?;

		if !word.is_empty() {
			items.push(IdxEntry { word, offset, size })
		}
	}
	Ok(items)
}

fn load_syn(vec: &Vec<IdxEntry>, items: &mut HashMap<String, IdxEntry>, syn: PathBuf) -> Result<()> {
	let file = File::open(syn)
		.map_err(|e| Error::FailedOpenFile("syn", e))?;
	let mut reader = BufReader::new(file);

	let mut buf: Vec<u8> = Vec::new();
	loop {
		buf.clear();
		let read_bytes = reader.read_until(0, &mut buf)
			.map_err(|e| Error::FailedOpenFile("syn", e))?;
		if let Some(b'\0') = buf.last() {
			buf.pop();
		}
		if read_bytes == 0 {
			break;
		}

		let word = buf_to_string(&buf);

		let mut b = [0; 4];
		if let Err(_) = reader.read(&mut b) {
			return Err(Error::InvalidSynIndex(word));
		}
		let index = u32::from_be_bytes(b) as usize;
		if !word.is_empty() {
			if let Some(entry) = vec.get(index) {
				items.insert(word.to_lowercase(),
					IdxEntry { word, offset: entry.offset, size: entry.size });
			};
		}
	}
	Ok(())
}
