use crate::error::{Error, Result};
use crate::ifo::{Ifo, Version};

use std::fmt::Debug;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use flate2::read::GzDecoder;
use std::collections::{HashMap, HashSet};
use byteorder::{BigEndian, ReadBytesExt};
use crate::buf_to_string;

struct IdxRawEntry {
	word: String,
	offset: usize,
	size: usize,
}

#[derive(Debug, Clone)]
pub struct IdxEntryBlock {
	pub offset: usize,
	pub size: usize,
}

#[derive(Debug)]
pub struct IdxEntry {
	pub word: String,
	pub blocks: Vec<IdxEntryBlock>,
}

impl IdxEntry {
	fn push_block(&mut self, offset: usize, size: usize)
	{
		self.blocks.push(IdxEntryBlock { offset, size })
	}
}

#[derive(Debug)]
pub struct Idx {
	pub(super) items: HashMap<String, IdxEntry>,
	pub(super) syn: Option<HashMap<String, HashSet<String>>>,
}

#[allow(unused)]
impl Idx {
	pub fn new(path: PathBuf, ifo: &Ifo, gz: bool, syn: Option<PathBuf>) -> Result<Idx>
	{
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

	pub fn lookup_blocks(&self, word: &str) -> Option<Vec<&IdxEntry>>
	{
		let lowercase_word = word.to_lowercase();
		let mut vec = vec![];
		let mut found = HashSet::new();
		if let Some(entry) = self.items.get(&lowercase_word) {
			vec.push(entry);
			found.insert(entry.word.clone());
		}
		if let Some(syn) = &self.syn {
			if let Some(alias) = syn.get(&lowercase_word) {
				for key in alias {
					if let Some(entry) = self.items.get(key) {
						if !found.contains(&entry.word) {
							vec.push(entry);
							found.insert(entry.word.clone());
						}
					}
				}
			}
		}
		if vec.len() == 0 {
			None
		} else {
			Some(vec)
		}
	}
}

#[inline]
fn read(version: &Version, idxoffsetbits: usize, reader: impl BufRead, syn: Option<PathBuf>) -> Result<Idx>
{
	let vec = match version {
		Version::V242 => read_items(reader, |r| Ok(r.read_u32::<BigEndian>()? as usize))?,
		Version::V300 => if idxoffsetbits == 64 {
			read_items(reader, |r| Ok(r.read_u64::<BigEndian>()? as usize))?
		} else {
			read_items(reader, |r| Ok(r.read_u32::<BigEndian>()? as usize))?
		}
	};
	let mut items = HashMap::new();
	vec.iter().for_each(|raw| {
		if raw.word.is_empty() {
			return;
		}
		let entry = items.entry(raw.word.to_lowercase())
			.or_insert(IdxEntry { word: raw.word.clone(), blocks: vec![] });
		entry.push_block(raw.offset, raw.size);
	});
	let syn = if let Some(syn) = syn {
		Some(load_syn(&vec, syn, &items)?)
	} else {
		None
	};
	Ok(Idx { items, syn })
}

fn read_items<F>(mut reader: impl BufRead, f: F) -> Result<Vec<IdxRawEntry>>
	where F: Fn(&mut dyn BufRead) -> std::io::Result<usize>
{
	let mut items = vec![];
	loop {
		let mut buf = vec![];
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

		items.push(IdxRawEntry { word, offset, size })
	}
	Ok(items)
}

fn load_syn(vec: &Vec<IdxRawEntry>, syn: PathBuf, items: &HashMap<String, IdxEntry>) -> Result<HashMap<String, HashSet<String>>>
{
	let file = File::open(syn)
		.map_err(|e| Error::FailedOpenFile("syn", e))?;
	let mut reader = BufReader::new(file);

	let mut syn = HashMap::new();
	loop {
		let mut buf = vec![];

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

		if !word.is_empty() {
			let index = u32::from_be_bytes(b) as usize;
			let lowercase_word = word.to_lowercase();
			if let Some(raw) = vec.get(index) {
				let alias = syn.entry(lowercase_word)
					.or_insert(HashSet::new());
				alias.insert(raw.word.to_lowercase());

				// setup the reverse alias if the alias exists in items
				let items_lowercase_key = word.to_lowercase();
				if items.contains_key(&items_lowercase_key) {
					let alias = syn.entry(raw.word.to_lowercase())
						.or_insert(HashSet::new());
					alias.insert(items_lowercase_key);
				}
			}
		}
	}

	Ok(syn)
}
