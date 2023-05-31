use crate::dict::Dict;
use crate::error::Result;
use crate::idx::Idx;
use crate::ifo::Ifo;

use std::path::PathBuf;
use crate::{StarDict, WordDefinition};

pub struct StarDictStd {
	path: PathBuf,

	pub ifo: Ifo,
	idx: Idx,
	dict: Dict,
}

impl StarDictStd {
	#[inline]
	pub(crate) fn new(path: PathBuf, ifo: Ifo, idx: PathBuf, idx_gz: bool,
		syn: Option<PathBuf>, dict: PathBuf, dict_bz: bool) -> Result<Self>
	{
		let idx = Idx::new(idx, &ifo, idx_gz, syn)?;
		let dict = Dict::new(dict, dict_bz)?;
		Ok(StarDictStd { path, ifo, idx, dict })
	}
}

impl StarDict for StarDictStd {
	#[inline]
	fn path(&self) -> &PathBuf {
		&self.path
	}

	#[inline]
	fn ifo(&self) -> &Ifo {
		&self.ifo
	}

	#[inline]
	fn lookup(&mut self, word: &str) -> Result<Option<Vec<WordDefinition>>> {
		let blocks = if let Some(blocks) = self.idx.lookup_blocks(word) {
			blocks
		} else {
			return Ok(None);
		};

		let mut definitions = vec![];
		for block in blocks {
			if let Some(result) = self.dict.get_definition(block, &self.ifo)? {
				definitions.push(result);
			}
		}
		Ok(Some(definitions))
	}
}
