#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Error with storage subsystem: {0}")]
	Storage(#[from] crate::storage::Error),
	#[error("Error with upstream registry: {0}")]
	Upstream(#[from] dkregistry::errors::Error),
	#[error("Not found")]
	InvalidDigest,
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
	#[error("JSON error: {0}")]
	Json(#[from] serde_json::Error)
}

impl actix_web::ResponseError for Error {
}

