use core::time::Duration;

use actix_web::HttpResponse;
use bytes::Bytes;
use clap::Subcommand;
use dkregistry::mediatypes::MediaTypes;
use futures::stream::BoxStream;
use futures::stream::TryStream;
use serde::Deserialize;
use serde::Serialize;

mod error;
pub mod filesystem;
pub mod s3;

pub use error::Error;

#[derive(Clone, Debug, Subcommand)]
pub enum StorageConfig {
	S3(s3::Config),
	Filesystem(filesystem::Config)
}

impl StorageConfig {
	pub fn repository(&self) -> Repository {
		match self {
			Self::S3(config) => Repository::S3(config.repository()),
			Self::Filesystem(config) => Repository::Filesystem(config.repository())
		}
	}
}

#[derive(Clone)]
pub enum Repository {
	S3(s3::Repository),
	Filesystem(filesystem::Repository)
}

pub struct ReadStream {
	length: u64,
	inner: BoxStream<'static, Result<Bytes, std::io::Error>>
}

impl ReadStream {
	pub fn new(length: u64, inner: BoxStream<'static, Result<Bytes, std::io::Error>>) -> Self {
		Self { length, inner }
	}

	pub fn into_inner(self) -> BoxStream<'static, Result<Bytes, std::io::Error>> {
		self.inner
	}
}

impl From<ReadStream> for HttpResponse {
	fn from(stream: ReadStream) -> Self {
		let mut response = HttpResponse::Ok();
		response.insert_header((actix_web::http::header::CONTENT_LENGTH, stream.length.to_string()));
		response.streaming(stream.inner)
	}
}

impl Repository {
	pub async fn read(self, object: &str, invalidation: Duration) -> Result<ReadStream, Error> {
		let result = match self {
			Self::S3(r) => r.read(object, invalidation).await?,
			Self::Filesystem(r) => r.read(object.into(), invalidation).await?
		};
		Ok(result)
	}

	pub async fn write<S, E>(&self, object: &str, reader: S, length: i64) -> Result<(), Error>
	where
		S: TryStream<Ok = Bytes, Error = E> + Unpin + Send + 'static,
		E: std::error::Error + Send + Sync + 'static,
		Error: From<E>
	{
		#[rustfmt::skip]
		#[allow(clippy::let_unit_value)] // Because it's likely that we will change the return type eventually, it'll require fewer changes, and it's harmless as-is.
		let result = match self {
			Self::S3(r) => r.write(object, reader, length).await?,
			Self::Filesystem(r) => r.write(object.into(), reader).await?
		};
		Ok(result)
	}
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
	pub manifest: Bytes,
	pub media_type: MediaTypes,
	pub digest: Option<String>
}

impl Manifest {
	pub fn new(manifest: Bytes, media_type: MediaTypes, digest: Option<String>) -> Self {
		Self { manifest, media_type, digest }
	}
}
