#[derive(Debug, thiserror::Error)]
pub enum GetObjectAge {
	#[error("Failed to get object from S3: {0}")]
	Rusoto(#[from] rusoto_core::RusotoError<rusoto_s3::GetObjectError>),
	#[error("Failed to parse datetime: {0}")]
	ParseTime(#[from] time::error::Parse)
}

