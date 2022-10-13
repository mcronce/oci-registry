use rusoto_core::RusotoError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
	#[error("Failed to get object from S3: {0}")]
	RusotoGet(#[from] RusotoError<rusoto_s3::GetObjectError>),
	#[error("Failed to put object into S3: {0}")]
	RusotoPut(#[from] RusotoError<rusoto_s3::PutObjectError>),
	#[error("Failed to get age of S3 object: {0}")]
	ObjectAge(#[from] super::s3::GetObjectAgeError)
}

