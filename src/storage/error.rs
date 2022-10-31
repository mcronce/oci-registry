use arcerror::ArcError;
use rusoto_core::RusotoError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
	#[error("Failed to get object from S3: {0:?}")]
	RusotoGet(#[from] RusotoError<rusoto_s3::GetObjectError>),
	#[error("Failed to put object into S3: {0:?}")]
	RusotoPut(#[from] RusotoError<rusoto_s3::PutObjectError>),
	#[error("Failed to parse datetime: {0}")]
	ParseTime(#[from] time::error::Parse),
	#[error("Object too old: {0}")]
	ObjectTooOld(humantime::Duration),
	#[error("Error reading from upstream: {0}")]
	Upstream(#[from] ArcError<dkregistry::errors::Error>)
}
