use core::time::Duration;
use std::time::SystemTime;

use actix_web::body::SizedStream;
use bytes::Bytes;
use clap::Subcommand;
use compact_str::format_compact;
use dkregistry::mediatypes::MediaTypes;
use futures::stream::BoxStream;
use futures::stream::TryStream;
use futures::stream::TryStreamExt;
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

impl From<ReadStream> for SizedStream<BoxStream<'static, Result<Bytes, Box<dyn std::error::Error + 'static>>>> {
	fn from(stream: ReadStream) -> Self {
		SizedStream::new(stream.length, Box::pin(stream.inner.err_into()))
	}
}

impl Repository {
	pub async fn read(&self, object: &str, invalidation: Duration) -> Result<ReadStream, Error> {
		let result = match self {
			Self::S3(r) => r.read(object, invalidation).await?,
			Self::Filesystem(r) => r.read(object.into(), invalidation).await?
		};
		Ok(result)
	}

	pub async fn write<S, E>(&self, object: &str, reader: S, length: i64) -> Result<(), Error>
	where
		S: TryStream<Ok = Bytes, Error = E> + Unpin + Send + 'static,
		E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
		Error: From<E>
	{
		#[allow(clippy::let_unit_value)] // Because it's likely that we will change the return type eventually, it'll require fewer changes, and it's harmless as-is.
		let result = match self {
			Self::S3(r) => r.write(object, reader, length).await?,
			Self::Filesystem(r) => r.write(object.into(), reader).await?
		};
		Ok(result)
	}

	pub async fn delete(&self, object: &str) -> Result<(), Error> {
		match self {
			Self::S3(r) => r.delete(object).await?,
			Self::Filesystem(r) => r.delete(object.as_ref()).await?
		};
		Ok(())
	}

	pub async fn delete_old_blobs(&self, older_than: SystemTime) -> Result<usize, Error> {
		match self {
			Self::S3(r) => r.delete_old_objects(older_than, "blobs/").await,
			Self::Filesystem(r) => r.delete_old_files(older_than, "blobs".as_ref()).await
		}
	}

	pub async fn delete_old_manifests(&self, ns: &str, older_than: SystemTime) -> Result<usize, Error> {
		let prefix = format_compact!("manifests/{ns}");
		let prefix: &str = prefix.as_ref();
		match self {
			Self::S3(r) => r.delete_old_objects(older_than, prefix).await,
			Self::Filesystem(r) => r.delete_old_files(older_than, prefix.as_ref()).await
		}
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
