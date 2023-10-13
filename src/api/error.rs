use actix_web::body::BoxBody;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use actix_web::HttpResponseBuilder;
use dkregistry::errors::Error as Upstream;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Error with storage subsystem: {0}")]
	Storage(#[from] crate::storage::Error),
	#[error("Error with upstream registry: {0}")]
	Upstream(#[from] Upstream),
	#[error("Not found")]
	InvalidDigest,
	#[error("Missing Content-Length header from upstream")]
	MissingContentLength,
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
	#[error("JSON error: {0}")]
	Json(#[from] serde_json::Error)
}

impl actix_web::ResponseError for Error {
	fn status_code(&self) -> StatusCode {
		match self {
			Self::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
			Self::Upstream(e) => match e {
				Upstream::UnexpectedHttpStatus(StatusCode::NOT_FOUND) => StatusCode::NOT_FOUND,
				Upstream::UnexpectedHttpStatus(_) => StatusCode::INTERNAL_SERVER_ERROR,
				Upstream::Client { status: StatusCode::NOT_FOUND } => StatusCode::NOT_FOUND,
				Upstream::Client { .. } => StatusCode::INTERNAL_SERVER_ERROR,
				_ => StatusCode::INTERNAL_SERVER_ERROR
			},
			Self::InvalidDigest => StatusCode::NOT_FOUND,
			Self::MissingContentLength => StatusCode::INTERNAL_SERVER_ERROR,
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

pub fn should_retry_without_namespace(err: &Upstream) -> bool {
	matches!(err, dkregistry::errors::Error::Reqwest(_) | dkregistry::errors::Error::UnexpectedHttpStatus(_) | dkregistry::errors::Error::Client { .. })
}
