pub mod error;
mod stardict;
mod idx;
mod ifo;
mod dict;

pub type Stardict = stardict::StarDict;
pub type LookupResult<'a> = stardict::LookupResult<'a>;