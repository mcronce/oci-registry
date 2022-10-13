use core::time::Duration;
use std::str::FromStr;
use std::time::SystemTime;

use actix_web::web::Bytes;
use clap::Parser;
use futures::stream::BoxStream;
use futures::stream::TryStream;
use futures::stream::TryStreamExt;
use rusoto_core::request::HttpClient;
use rusoto_core::ByteStream;
use rusoto_core::Region;
use rusoto_core::RusotoError;
use rusoto_credential::StaticProvider;
use rusoto_s3::GetObjectError;
use rusoto_s3::GetObjectOutput;
use rusoto_s3::GetObjectRequest;
use rusoto_s3::PutObjectRequest;
use rusoto_s3::S3;
use rusoto_s3::S3Client;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

mod error;
pub use error::GetObjectAge as GetObjectAgeError;

#[derive(Clone, Debug, Parser)]
pub struct S3Config {
	#[clap(env, long)]
	s3_host: Option<String>,
	#[clap(env, long)]
	s3_access_key: String,
	#[clap(env, long)]
	s3_secret_key: String,
	#[clap(env, long, default_value = "us-east-1")]
	s3_region: String,
	#[clap(env, long)]
	s3_bucket: String
}

impl S3Config {
	pub fn repository(&self) -> Repository {
		let region = match self.s3_host.clone() {
			Some(s) => Region::Custom{
				name: self.s3_region.clone(),
				endpoint: s.clone()
			},
			None => Region::from_str(&self.s3_region).unwrap()
		};
		let creds = StaticProvider::new(self.s3_access_key.clone(), self.s3_secret_key.clone(), None, None);
		let http = HttpClient::new().unwrap();
		Repository{
			inner: S3Client::new_with(http, creds, region),
			bucket: self.s3_bucket.clone()
		}
	}
}

#[derive(Clone)]
pub struct Repository {
	inner: S3Client,
	bucket: String
}

impl Repository {
	async fn get_object(&self, object: &str) -> Result<GetObjectOutput, RusotoError<GetObjectError>> {
		let req = GetObjectRequest{
			bucket: self.bucket.clone(),
			key: object.into(),
			..Default::default()
		};
		self.inner.get_object(req).await
	}

	pub async fn age(&self, object: &str) -> Result<Duration, GetObjectAgeError> {
		let obj = self.get_object(object).await?;
		let time = OffsetDateTime::parse(&obj.last_modified.unwrap(), &Rfc3339)?;
		Ok((SystemTime::now() - time).try_into().unwrap_or_default())
	}

	pub async fn read(self, object: &str) -> Result<BoxStream<'static, Result<Bytes, std::io::Error>>, RusotoError<GetObjectError>> {
		let obj = self.get_object(&object).await?;
		Ok(Box::pin(obj.body.unwrap()))
	}

	pub async fn write<S, E>(&self, object: &str, reader: S, length: i64) -> Result<(), super::Error>
	where
		S: TryStream<Ok = Bytes, Error = E> + Unpin + Send + 'static,
		E: std::error::Error + Send + Sync + 'static,
		super::Error: From<E>
	{
		let req = PutObjectRequest{
			bucket: self.bucket.clone(),
			key: object.into(),
			content_length: Some(length),
			body: Some(ByteStream::new(reader.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))),
			..Default::default()
		};
		self.inner.put_object(req).await?;
		Ok(())
	}
}

