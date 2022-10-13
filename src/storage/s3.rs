use core::time::Duration;
use std::str::FromStr;
use std::time::SystemTime;

use actix_web::web::Bytes;
use clap::Parser;
use futures::stream::BoxStream;
use futures::stream::Stream;
use futures::stream::StreamExt;
use rusoto_core::request::HttpClient;
use rusoto_core::ByteStream;
use rusoto_core::Region;
use rusoto_core::RusotoError;
use rusoto_credential::StaticProvider;
use rusoto_s3::GetObjectError;
use rusoto_s3::GetObjectOutput;
use rusoto_s3::GetObjectRequest;
use rusoto_s3::PutObjectError;
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

	pub async fn check_if_exists(&self, object: &str) -> Result<bool, RusotoError<GetObjectError>> {
		match self.get_object(object).await {
			Ok(_) => Ok(true),
			Err(RusotoError::Service(GetObjectError::NoSuchKey(_))) => Ok(false),
			Err(e) => Err(e)
		}
	}

	pub async fn age(&self, object: &str) -> Result<Duration, GetObjectAgeError> {
		let obj = self.get_object(object).await?;
		let time = OffsetDateTime::parse(&obj.last_modified.unwrap(), &Rfc3339)?;
		Ok((SystemTime::now() - time).try_into().unwrap_or_default())
	}

	pub async fn read(&self, object: &str) -> Result<BoxStream<Result<Bytes, std::io::Error>>, RusotoError<GetObjectError>> {
		let obj = self.get_object(object).await?;
		Ok(Box::pin(obj.body.unwrap()))
	}

	pub async fn write(&self, object: &str, reader: impl Stream<Item = Result<&[u8], std::io::Error>> + Unpin + Send + 'static) -> Result<(), RusotoError<PutObjectError>> {
		let req = PutObjectRequest{
			bucket: self.bucket.clone(),
			key: object.into(),
			body: Some(ByteStream::new(reader.map(|r| r.map(Bytes::copy_from_slice)))),
			..Default::default()
		};
		self.inner.put_object(req).await?;
		Ok(())
	}
}

