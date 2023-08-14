use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
	#[error("Failed to open ifo file")]
	FailedOpenIfo(std::io::Error),

	#[error("Dict path is not invalid")]
	InvalidDictPath,

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

	#[error("Invalid idx block: {0}")]
	InvalidIdxBlock(String),

	#[error("Invalid syn index for {0}")]
	InvalidSynIndex(String),

	#[error("Invalid dict content")]
	InvalidDictContent,

	#[error("Invalid dict file")]
	InvalidDict,

	#[error("Invalid dict field: {0}")]
	FailedParseDictHeader(&'static str),

	#[error("Error when reading: {0}")]
	FailedReadHeader(#[from] std::io::Error),

	#[error("No {0} resource found")]
	NoResourceFound(String),

	#[error("Load resource {0} failed: {1}")]
	FailedLoadResource(String, String),

	#[error("Failed get user cache folder")]
	NoCacheDir,

	#[error("Failed open cache: {0}")]
	FailedOpenCache(String),

	#[error("Initiating dict cache")]
	CacheInitiating,

	#[error("Invalid dictionary cache: {0}, remove and build it again")]
	InvalidDictCache(String),
}

pub type Result<T> = std::result::Result<T, Error>;
