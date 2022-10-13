use core::time::Duration;

use actix_web::web::Bytes;
use clap::Subcommand;
use futures::stream::Stream;
use futures::stream::BoxStream;

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

pub enum Repository {
	S3(s3::Repository),
	Filesystem(filesystem::Repository)
}

impl Repository {
	pub async fn check_if_exists(&self, object: &str) -> Result<bool, Error> {
		let result = match self {
			Self::S3(r) => r.check_if_exists(object).await?,
			Self::Filesystem(r) => r.check_if_exists(object.into()).await?
		};
		Ok(result)
	}

	pub async fn age(&self, object: &str) -> Result<Duration, Error> {
		let result = match self {
			Self::S3(r) => r.age(object).await?,
			Self::Filesystem(r) => r.age(object.into()).await?
		};
		Ok(result)
	}

	pub async fn read(&self, object: &str) -> Result<BoxStream<Result<Bytes, std::io::Error>>, Error> {
		let result = match self {
			Self::S3(r) => r.read(object).await?,
			Self::Filesystem(r) => r.read(object.into()).await?
		};
		Ok(result)
	}

	pub async fn write(&self, object: &str, reader: impl Stream<Item = Result<&[u8], std::io::Error>> + Unpin + Send + 'static) -> Result<(), Error> {
		let result = match self {
			Self::S3(r) => r.write(object, reader).await?,
			Self::Filesystem(r) => r.write(object.into(), reader).await?
		};
		Ok(result)
	}
}

