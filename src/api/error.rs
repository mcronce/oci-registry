use actix_web::body::BoxBody;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use actix_web::HttpResponseBuilder;
use tracing::error;

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
	fn status_code(&self) -> StatusCode {
		match self {
			Self::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
			Self::Upstream(_) => StatusCode::INTERNAL_SERVER_ERROR,
			Self::InvalidDigest => StatusCode::NOT_FOUND,
			Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
			Self::Json(_) => StatusCode::INTERNAL_SERVER_ERROR
		}
	}

	fn error_response(&self) -> HttpResponse<BoxBody> {
		let status_code = self.status_code();
		error!("{}: {}", status_code.as_u16(), self);
		HttpResponseBuilder::new(status_code).body(self.to_string())
	}
}
