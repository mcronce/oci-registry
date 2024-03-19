use arcerror::ArcError;
use rusoto_core::RusotoError;

use crate::api::stream::DigestMismatchError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
	#[error("I/O error: {0}")]
	Io(ArcError<std::io::Error>),
	#[error("Failed to list objects in S3: {0:?}")]
	RusotoList(ArcError<RusotoError<rusoto_s3::ListObjectsV2Error>>),
	#[error("Failed to get object from S3: {0:?}")]
	RusotoGet(ArcError<RusotoError<rusoto_s3::GetObjectError>>),
	#[error("Failed to put object into S3: {0:?}")]
	RusotoPut(ArcError<RusotoError<rusoto_s3::PutObjectError>>),
	#[error("Failed to delete object from S3: {0:?}")]
	RusotoDelete(ArcError<RusotoError<rusoto_s3::DeleteObjectError>>),
	#[error("Failed to parse datetime: {0}")]
	ParseTime(#[from] time::error::Parse),
	#[error("Object too old: {0}")]
	ObjectTooOld(humantime::Duration),
	#[error("Error reading from upstream: {0}")]
	Upstream(ArcError<dkregistry::errors::Error>),
	#[error("{0}")]
	DataCorrupt(#[from] DigestMismatchError)
}

impl From<std::io::Error> for Error {
	#[inline]
	fn from(inner: std::io::Error) -> Self {
		Self::Io(ArcError::from(inner))
	}
}

impl From<RusotoError<rusoto_s3::ListObjectsV2Error>> for Error {
	#[inline]
	fn from(inner: RusotoError<rusoto_s3::ListObjectsV2Error>) -> Self {
		Self::RusotoList(ArcError::from(inner))
	}
}

impl From<RusotoError<rusoto_s3::GetObjectError>> for Error {
	#[inline]
	fn from(inner: RusotoError<rusoto_s3::GetObjectError>) -> Self {
		Self::RusotoGet(ArcError::from(inner))
	}
}

impl From<RusotoError<rusoto_s3::PutObjectError>> for Error {
	#[inline]
	fn from(inner: RusotoError<rusoto_s3::PutObjectError>) -> Self {
		Self::RusotoPut(ArcError::from(inner))
	}
}

impl From<RusotoError<rusoto_s3::DeleteObjectError>> for Error {
	#[inline]
	fn from(inner: RusotoError<rusoto_s3::DeleteObjectError>) -> Self {
		Self::RusotoDelete(ArcError::from(inner))
	}
}

impl From<dkregistry::errors::Error> for Error {
	#[inline]
	fn from(inner: dkregistry::errors::Error) -> Self {
		Self::Upstream(ArcError::from(inner))
	}
}
