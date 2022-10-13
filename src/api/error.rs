#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Error with upstream registry: {0}")]
	Upstream(#[from] dkregistry::errors::Error),
	#[error("Not found")]
	InvalidDigest
}

impl actix_web::ResponseError for Error {
}

