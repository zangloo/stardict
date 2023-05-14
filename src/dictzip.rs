use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use inflate::inflate_bytes;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use byteorder::{LE, ReadBytesExt};
use crate::buf_to_string;
use crate::error::{Error, Result};

struct DictZipHeader {
	id: u16,
	compression_method: u8,
	flags: u8,
	#[allow(unused)]
	modification_time: u32,
	#[allow(unused)]
	extra_flags: u8,
	#[allow(unused)]
	os: u8,
}

#[allow(unused)]
const HEADER_FLAG_TEXT: u8 = 0b00000001;
const HEADER_FLAG_CRC: u8 = 0b00000010;
const HEADER_FLAG_EXTRA: u8 = 0b00000100;
const HEADER_FLAG_NAME: u8 = 0b00001000;
const HEADER_FLAG_COMMENT: u8 = 0b00010000;

const GZIP_ID: u16 = 0x8B1F;
const COMPRESSION_METHOD_DEFLATE: u8 = 0x08;
const RA_ID: u16 = 0x4152;

pub struct DictZip {
	#[allow(unused)]
	reader: BufReader<File>,

	#[allow(unused)]
	header: DictZipHeader,
	chunk_length: usize,
	chunks: Vec<u16>,
	data_offset: u64,
	cache: HashMap<usize, Vec<u8>>,

	#[allow(unused)]
	filename: Option<String>,
	#[allow(unused)]
	comment: Option<String>,
	#[allow(unused)]
	crc: Option<u16>,
}

impl DictZip {
	pub fn new(mut reader: BufReader<File>) -> Result<DictZip> {
		let header = read_header(&mut reader).map_err(|_| Error::InvalidDict)?;
		if header.id != GZIP_ID {
			return Err(Error::FailedParseDictHeader("header id"));
		}
		if header.compression_method != COMPRESSION_METHOD_DEFLATE {
			return Err(Error::FailedParseDictHeader("header compress method"));
		}
		if header.flags & HEADER_FLAG_EXTRA == 0 {
			return Err(Error::FailedParseDictHeader("header flag extra not set"));
		}

		let (chunk_length, chunks) = read_chunks(&mut reader).map_err(|_| Error::InvalidDict)?
			.ok_or(Error::FailedParseDictHeader("header with no extra ra field"))?;

		let filename = if header.flags & HEADER_FLAG_NAME == 0 {
			None
		} else {
			Some(read_string(&mut reader)?)
		};
		let comment = if header.flags & HEADER_FLAG_COMMENT == 0 {
			None
		} else {
			Some(read_string(&mut reader)?)
		};
		let crc = if header.flags & HEADER_FLAG_CRC == 0 {
			None
		} else {
			Some(reader.read_u16::<LE>()?)
		};
		let data_offset = reader.stream_position()?;
		let cache = HashMap::new();
		let dict = DictZip {
			reader,
			header,
			chunk_length,
			chunks,
			data_offset,
			cache,
			filename,
			comment,
			crc,
		};
		Ok(dict)
	}

	pub fn get_text(&mut self, offset: usize, size: usize) -> Option<Cow<str>>
	{
		let chunk_count = self.chunks.len();
		let first_chunk = offset / self.chunk_length;
		if first_chunk > chunk_count {
			return None;
		}
		let last_chunk = (offset + size - 1) / self.chunk_length;
		if last_chunk >= chunk_count {
			return None;
		}

		let mut buf = vec![];
		let chunk_offset = offset - first_chunk * self.chunk_length;
		for i in first_chunk..=last_chunk {
			let chunk = self.read_chunk(i)?;
			buf = [buf.as_slice(), chunk.as_slice()].concat();
		}
		let segment = &buf[chunk_offset..chunk_offset + size];
		let text = buf_to_string(segment);
		Some(Cow::Owned(text))
	}

	fn read_chunk(&mut self, chunk_index: usize) -> Option<&Vec<u8>> {
		if !self.cache.contains_key(&chunk_index) {
			let mut offset = self.data_offset;
			for i in 0..chunk_index {
				offset += *self.chunks.get(i).unwrap() as u64;
			}
			self.reader.seek(SeekFrom::Start(offset)).ok()?;
			let length = *self.chunks.get(chunk_index)? as usize;
			let mut buf = vec![0; length];
			self.reader.read_exact(&mut buf).ok()?;

			let text_buf = inflate_bytes(&buf).ok()?;
			self.cache.insert(chunk_index, text_buf);
		}

		self.cache.get(&chunk_index)
	}
}

#[inline]
fn read_string(reader: &mut (impl BufRead + Seek)) -> Result<String> {
	let mut buf = vec![];
	reader.read_until(0, &mut buf)?;
	if let Some(0) = buf.last() {
		buf.pop();
	}
	String::from_utf8(buf).map_err(|_| Error::FailedParseDictHeader("Incorrect string"))
}

fn read_chunks(reader: &mut (impl BufRead + Seek)) -> Result<Option<(usize, Vec<u16>)>> {
	#[inline]
	fn read_u16(reader: &mut impl BufRead, bytes_read: &mut u16) -> Result<u16> {
		*bytes_read += 2;
		Ok(reader.read_u16::<LE>()?)
	}
	let extra_len = reader.read_u16::<LE>()?;
	let mut bytes_read = 0;
	while bytes_read < extra_len {
		let field_id = read_u16(reader, &mut bytes_read)?;
		if field_id == RA_ID {
			break;
		}
		let sub_field_length = read_u16(reader, &mut bytes_read)?;
		reader.seek(SeekFrom::Current(sub_field_length as i64))?;
		bytes_read += sub_field_length;
	}
	if extra_len - bytes_read == 0 {
		return Err(Error::FailedParseDictHeader("Failed to find RA data!"));
	}
	let mut ra_read = 0;
	let ra_size = read_u16(reader, &mut bytes_read)?;
	let version = read_u16(reader, &mut ra_read)?;
	if version != 1 {
		return Err(Error::FailedParseDictHeader("Incorrect ra version"));
	}
	let chunk_length = read_u16(reader, &mut ra_read)? as usize;
	let chunk_count = read_u16(reader, &mut ra_read)?;
	if (ra_size - ra_read) != chunk_count * 2 {
		return Err(Error::FailedParseDictHeader("Subfield size remaining too small for chunk count"));
	}
	let mut chunks = Vec::with_capacity(chunk_count as usize);
	for _i in 0..chunk_count {
		let chunk = read_u16(reader, &mut ra_read)?;
		chunks.push(chunk);
	}

	// skip through the rest of extraData
	let bytes_to_skips = extra_len - bytes_read - ra_read;
	if bytes_to_skips > 0 {
		reader.seek(SeekFrom::Current(bytes_to_skips as i64))?;
	}

	Ok(Some((chunk_length, chunks)))
}

fn read_header(reader: &mut impl BufRead) -> Result<DictZipHeader> {
	let id = reader.read_u16::<LE>()?;
	let compression_method = reader.read_u8()?;
	let flags = reader.read_u8()?;
	let modification_time = reader.read_u32::<LE>()?;
	let extra_flags = reader.read_u8()?;
	let os = reader.read_u8()?;

	let header = DictZipHeader {
		id,
		compression_method,
		flags,
		modification_time,
		extra_flags,
		os,
	};
	Ok(header)
}