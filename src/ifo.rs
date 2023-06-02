use crate::error::{Error, Result};

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Clone,Debug)]
pub enum Version {
	V242,
	V300,
}

/// bookname=      // required
/// wordcount=     // required
/// synwordcount=  // required if ".syn" file exists.
/// idxfilesize=   // required
/// idxoffsetbits= // New in 3.0.0
/// author=
/// email=
/// website=
/// description=	// You can use <br> for new line.
/// date=
/// sametypesequence= // very important.
/// dicttype=
#[derive(Clone, Debug)]
pub struct Ifo {
	pub version: Version,
	pub bookname: String,
	pub wordcount: usize,
	pub synwordcount: usize,
	pub idxfilesize: usize,
	pub idxoffsetbits: usize,
	pub author: String,
	pub email: String,
	pub website: String,
	pub description: String,
	pub date: String,
	pub sametypesequence: String,
	pub dicttype: String,
}

#[allow(unused)]
impl Ifo {
	pub fn new(path: PathBuf) -> Result<Ifo> {
		let mut ifo = Ifo {
			version: Version::V242,
			bookname: String::new(),
			wordcount: 0,
			synwordcount: 0,
			idxfilesize: 0,
			idxoffsetbits: 32,
			author: String::new(),
			email: String::new(),
			website: String::new(),
			description: String::new(),
			date: String::new(),
			sametypesequence: String::new(),
			dicttype: String::new(),
		};

		let lines = BufReader::new(
			File::open(path)
				.map_err(|e| Error::FailedOpenFile("ifo", e))?)
			.lines();
		for line in lines {
			let line = line.map_err(|e| Error::FailedOpenFile("ifo", e))?;
			if let Some(id) = line.find('=') {
				let key = &line[..id];
				let val = String::from(&line[id + 1..]);
				match key {
					"version" =>
						match val.as_str() {
							"2.4.2" => {}
							"3.0.0" => ifo.version = Version::V300,
							_ => return Err(Error::InvalidVersion(val)),
						},
					"bookname" => ifo.bookname = val,
					"wordcount" => ifo.wordcount = val.parse()
						.map_err(|_| Error::InvalidIfoValue("wordcount"))?,
					"synwordcount" =>
						ifo.synwordcount = val.parse()
							.map_err(|_| Error::InvalidIfoValue("synwordcount"))?,
					"idxfilesize" =>
						ifo.idxfilesize = val.parse()
							.map_err(|_| Error::InvalidIfoValue("idxfilesize"))?,
					"idxoffsetbits" =>
						ifo.idxoffsetbits = val.parse()
							.map_err(|_| Error::InvalidIfoValue("idxoffsetbits"))?,
					"author" => ifo.author = val,
					"email" => ifo.email = val,
					"website" => ifo.website = val,
					"description" => ifo.description = val,
					"date" => ifo.date = val,
					"sametypesequence" => ifo.sametypesequence = val,
					"dicttype" => ifo.dicttype = val,
					_ => (),
				};
			}
		}
		Ok(ifo)
	}
}
