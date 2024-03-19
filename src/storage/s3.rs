use core::pin::Pin;
use core::time::Duration;
use std::str::FromStr;
use std::time::SystemTime;
use std::vec::IntoIter;

use actix_web::web::Bytes;
use clap::Parser;
use compact_str::CompactString;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use futures::stream::Stream;
use futures::stream::StreamExt;
use futures::stream::TryStream;
use futures::stream::TryStreamExt;
use futures::task::Context;
use futures::task::Poll;
use rusoto_core::request::HttpClient;
use rusoto_core::ByteStream;
use rusoto_core::Region;
use rusoto_core::RusotoError;
use rusoto_credential::StaticProvider;
use rusoto_s3::DeleteObjectError;
use rusoto_s3::DeleteObjectRequest;
use rusoto_s3::GetObjectError;
use rusoto_s3::GetObjectOutput;
use rusoto_s3::GetObjectRequest;
use rusoto_s3::ListObjectsV2Error;
use rusoto_s3::ListObjectsV2Output;
use rusoto_s3::ListObjectsV2Request;
use rusoto_s3::PutObjectRequest;
use rusoto_s3::S3Client;
use rusoto_s3::S3;
use time::format_description::well_known::Rfc2822;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::info;

use super::ReadStream;

#[derive(Clone, Debug, Parser)]
pub struct Config {
	#[clap(env = "S3_HOST", long)]
	host: Option<String>,
	#[clap(env = "S3_ACCESS_KEY", long)]
	access_key: CompactString,
	#[clap(env = "S3_SECRET_KEY", long)]
	secret_key: String,
	#[clap(env = "S3_REGION", long, default_value = "us-east-1")]
	region: CompactString,
	#[clap(env = "S3_BUCKET", long)]
	bucket: CompactString
}

impl Config {
	pub fn repository(&self) -> Repository {
		let region = match self.host.clone() {
			Some(s) => Region::Custom { name: self.region.to_string(), endpoint: s },
			None => Region::from_str(&self.region).unwrap()
		};
		let creds = StaticProvider::new(self.access_key.to_string(), self.secret_key.clone(), None, None);
		let http = HttpClient::new().unwrap();
		Repository {
			inner: S3Client::new_with(http, creds, region),
			bucket: self.bucket.clone()
		}
	}
}

struct ListObjectsStream {
	client: S3Client,
	bucket: CompactString,
	current_continuation_token: Option<String>,
	current_contents: IntoIter<rusoto_s3::Object>,
	current_future: Option<BoxFuture<'static, Result<ListObjectsV2Output, RusotoError<ListObjectsV2Error>>>>
}

impl Stream for ListObjectsStream {
	type Item = Result<rusoto_s3::Object, RusotoError<ListObjectsV2Error>>;

	#[rustfmt::skip]
	fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		if let Some(obj) = self.current_contents.next() {
			return Poll::Ready(Some(Ok(obj)));
		}
		if let Some(mut future) = self.current_future.take() {
			let output = match future.poll_unpin(ctx) {
				Poll::Ready(Ok(v)) => v,
				Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
				Poll::Pending => {
					self.current_future = Some(future);
					return Poll::Pending;
				}
			};
			self.current_continuation_token = output.continuation_token;
			self.current_contents = match output.contents {
				Some(v) => v.into_iter(),
				None => vec![].into_iter()
			};
			ctx.waker().wake_by_ref();
			return Poll::Pending;
		}
		let Some(token) = self.current_continuation_token.take() else {
			return Poll::Ready(None);
		};
		self.current_future = {
			let client = Box::pin(self.client.clone());
			let bucket = self.bucket.to_string();
			Some(Box::pin(async move {
				client.list_objects_v2(ListObjectsV2Request{
					bucket,
					continuation_token: Some(token),
					..Default::default()
				}).await
			}))
		};
		ctx.waker().wake_by_ref();
		Poll::Pending
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		match self.current_continuation_token.is_some() || self.current_future.is_some() {
			true => (self.current_contents.len(), None),
			false => (self.current_contents.len(), Some(self.current_contents.len()))
		}
	}
}

#[derive(Clone)]
pub struct Repository {
	inner: S3Client,
	bucket: CompactString
}

impl Repository {
	async fn list_objects(&self, prefix: &str) -> Result<ListObjectsStream, RusotoError<ListObjectsV2Error>> {
		let req = ListObjectsV2Request {
			bucket: self.bucket.to_string(),
			prefix: Some(prefix.into()),
			..Default::default()
		};
		let result = self.inner.list_objects_v2(req).await?;
		Ok(ListObjectsStream {
			client: self.inner.clone(),
			bucket: self.bucket.clone(),
			current_continuation_token: result.continuation_token,
			current_contents: result.contents.unwrap_or_default().into_iter(),
			current_future: None
		})
	}

	async fn get_object(&self, object: &str) -> Result<GetObjectOutput, RusotoError<GetObjectError>> {
		let req = GetObjectRequest {
			bucket: self.bucket.to_string(),
			key: object.into(),
			..Default::default()
		};
		self.inner.get_object(req).await
	}

	pub async fn read(&self, object: &str, invalidation: Duration) -> Result<ReadStream, super::Error> {
		let obj = self.get_object(object).await?;
		let time = obj.last_modified.map(|s| OffsetDateTime::parse(&s, &Rfc2822)).transpose()?.unwrap_or(OffsetDateTime::UNIX_EPOCH);
		let age = Duration::try_from(SystemTime::now() - time).unwrap_or_default();
		if (age > invalidation) {
			return Err(super::Error::ObjectTooOld(age.into()));
		}

		Ok(ReadStream::new(obj.content_length.unwrap().try_into().unwrap_or_default(), Box::pin(obj.body.unwrap())))
	}

	pub async fn write<S, E>(&self, object: &str, reader: S, length: i64) -> Result<(), super::Error>
	where
		S: TryStream<Ok = Bytes, Error = E> + Unpin + Send + 'static,
		E: std::error::Error + Send + Sync + 'static,
		super::Error: From<E>
	{
		let req = PutObjectRequest {
			bucket: self.bucket.to_string(),
			key: object.into(),
			content_length: Some(length),
			body: Some(ByteStream::new(reader.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))),
			..Default::default()
		};

		if let Err(e) = self.inner.put_object(req).await {
			self.delete(object).await?;
			return Err(e.into());
		}

		Ok(())
	}

	pub async fn delete(&self, object: &str) -> Result<(), RusotoError<DeleteObjectError>> {
		let req = DeleteObjectRequest {
			bucket: self.bucket.to_string(),
			key: object.to_owned(),
			..Default::default()
		};
		self.inner.delete_object(req).await?;
		Ok(())
	}

	pub async fn delete_old_objects(&self, older_than: SystemTime, prefix: &str) -> Result<usize, super::Error> {
		let mut count = 0;
		let mut stream = self.list_objects(prefix).await?;
		while let Some(obj) = stream.next().await {
			let obj = obj?;
			let Some(key) = obj.key else {
				continue;
			};
			let modified = obj.last_modified.and_then(|s| OffsetDateTime::parse(&s, &Rfc3339).ok()).unwrap_or(OffsetDateTime::UNIX_EPOCH);
			if (modified < older_than) {
				match self.delete(key.as_ref()).await {
					Ok(_) => info!(object = key, "Aged out"),
					Err(_) => continue
				};
				count += 1;
			}
		}
		Ok(count)
	}
}
