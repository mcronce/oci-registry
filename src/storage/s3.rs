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
use time::format_description::well_known::Rfc2822;

#[derive(Clone, Debug, Parser)]
pub struct S3Config {
	#[clap(env = "S3_HOST", long)]
	host: Option<String>,
	#[clap(env = "S3_ACCESS_KEY", long)]
	access_key: String,
	#[clap(env = "S3_SECRET_KEY", long)]
	secret_key: String,
	#[clap(env = "S3_REGION", long, default_value = "us-east-1")]
	region: String,
	#[clap(env = "S3_BUCKET", long)]
	bucket: String
}

impl S3Config {
	pub fn repository(&self) -> Repository {
		let region = match self.host.clone() {
			Some(s) => Region::Custom{
				name: self.region.clone(),
				endpoint: s.clone()
			},
			None => Region::from_str(&self.region).unwrap()
		};
		let creds = StaticProvider::new(self.access_key.clone(), self.secret_key.clone(), None, None);
		let http = HttpClient::new().unwrap();
		Repository{
			inner: S3Client::new_with(http, creds, region),
			bucket: self.bucket.clone()
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

	pub async fn read(self, object: &str, invalidation: Duration) -> Result<BoxStream<'static, Result<Bytes, std::io::Error>>, super::Error> {
		let obj = self.get_object(&object).await?;
		let time = OffsetDateTime::parse(&obj.last_modified.unwrap(), &Rfc2822)?;
		let age = Duration::try_from(SystemTime::now() - time).unwrap_or_default();
		if(age > invalidation) {
			return Err(super::Error::ObjectTooOld(age.into()));
		}

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

