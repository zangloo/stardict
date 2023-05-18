pub mod error;
mod stardict;
mod idx;
mod ifo;
mod dict;
mod dictzip;

pub use crate::stardict::StarDict;
pub use crate::stardict::WordDefinition;

#[inline]
fn buf_to_string(buf: &[u8]) -> String {
	String::from_utf8_lossy(buf)
		.chars()
		.filter(|&c| c != '\u{fffd}')
		.collect()
}
