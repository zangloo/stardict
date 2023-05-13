use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
	#[error("Failed to open ifo file")]
	FailedOpenIfo(std::io::Error),

	#[error("No {0} file found")]
	NoFileFound(&'static str),

	#[error("Failed open {0} file")]
	FailedOpenFile(&'static str, std::io::Error),

	#[error("Invalid version")]
	InvalidVersion(String),

	#[error("Invalid value {0} in ifo")]
	InvalidIfoValue(&'static str),

	#[error("Invalid idx element: {0}")]
	InvalidIdxElement(&'static str),

	#[error("Invalid syn index for {0}")]
	InvalidSynIndex(String),

	#[error("Invalid dict file")]
	InvalidDict,
}

pub type Result<T> = std::result::Result<T, Error>;
