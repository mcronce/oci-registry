use core::time::Duration;

use actix_web::web::Bytes;
use clap::Subcommand;
use futures::stream::TryStream;
use futures::stream::BoxStream;
use serde::Deserialize;
use serde::Serialize;

mod error;
pub mod filesystem;
pub mod s3;

pub use error::Error;

#[derive(Clone, Debug, Subcommand)]
pub enum StorageConfig {
	S3(s3::S3Config),
	Filesystem(filesystem::FilesystemConfig)
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

impl Repository {
	pub async fn age(&self, object: &str) -> Result<Duration, Error> {
		let result = match self {
			Self::S3(r) => r.age(object).await?,
			Self::Filesystem(r) => r.age(object.into()).await?
		};
		Ok(result)
	}

	pub async fn read(self, object: &str) -> Result<BoxStream<'static, Result<Bytes, std::io::Error>>, Error> {
		let result = match self {
			Self::S3(r) => r.read(object).await?,
			Self::Filesystem(r) => r.read(object.into()).await?
		};
		Ok(result)
	}

	pub async fn write<S, E>(&self, object: &str, reader: S, length: i64) -> Result<(), Error>
	where
		S: TryStream<Ok = Bytes, Error = E> + Unpin + Send + 'static,
		E: std::error::Error + Send + Sync + 'static,
		Error: From<E>
	{
		let result = match self {
			Self::S3(r) => r.write(object, reader, length).await?,
			Self::Filesystem(r) => r.write(object.into(), reader).await?
		};
		Ok(result)
	}
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
	pub manifest: dkregistry::v2::manifest::Manifest,
	pub digest: Option<String>
}

impl Manifest {
	pub fn new(manifest: dkregistry::v2::manifest::Manifest, digest: Option<String>) -> Self {
		Self{manifest, digest}
	}
}

