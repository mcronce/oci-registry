use std::iter;

use actix_web::body::SizedStream;
use actix_web::http;
use actix_web::http::header::HeaderName;
use actix_web::rt;
use actix_web::web;
use actix_web::HttpResponse;
use compact_str::CompactString;
use dkregistry::v2::Client;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use futures::Stream;
use once_cell::sync::Lazy;
use prometheus::register_histogram_vec;
use prometheus::register_int_counter_vec;
use prometheus::HistogramVec;
use prometheus::IntCounterVec;
use serde::Deserialize;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};
use tracing::error;
use tracing::warn;

use crate::image::ImageName;
use crate::image::ImageReference;
use crate::storage::Manifest;
use crate::storage::Repository;
use crate::upstream::Clients;

pub mod error;
use error::should_retry_without_namespace;
use error::Error;
pub mod stream;
use stream::DigestCheckedStream;

pub struct RequestConfig {
	repo: Repository,
	upstream: Mutex<Clients>,
	default_ns: CompactString,
	check_cache_digest: bool,
	blob_first_chunk_read_timeout: Duration,
	blob_first_chunk_write_timeout: Duration,
}

impl RequestConfig {
	pub fn new(repo: Repository, upstream: Clients, default_ns: CompactString, check_cache_digest: bool, blob_first_chunk_read_timeout: Duration, blob_first_chunk_write_timeout: Duration) -> Self {
		Self {
			repo,
			upstream: Mutex::new(upstream),
			default_ns,
			check_cache_digest,
			blob_first_chunk_read_timeout,
			blob_first_chunk_write_timeout,
		}
	}
}

async fn authenticate_with_upstream(upstream: &mut Client, scope: &str) -> Result<(), dkregistry::errors::Error> {
	upstream.authenticate(&[scope]).await?;
	Ok(())
}

pub async fn root(config: web::Data<RequestConfig>, qstr: web::Query<ManifestQueryString>) -> Result<&'static str, Error> {
	let mut upstream = { config.upstream.lock().await.get(qstr.ns.as_deref().unwrap_or_else(|| config.default_ns.as_ref()))?.client.clone() };
	upstream.authenticate(&[]).await?;
	Ok("")
}

#[derive(Debug, Deserialize)]
pub struct ManifestRequest {
	image: ImageName,
	reference: ImageReference,
}

impl ManifestRequest {
	fn http_path(&self) -> String {
		format!("/{}/manifests/{}", self.image, self.reference)
	}

	fn storage_path(&self, ns: &str) -> String {
		match self.image.as_ref().split('/').next() {
			Some(part) if part == ns => format!("manifests/{}/{}", self.image, self.reference),
			_ => format!("manifests/{}/{}/{}", ns, self.image, self.reference),
		}
	}
}

#[derive(Debug, Deserialize)]
pub struct ManifestQueryString {
	ns: Option<CompactString>,
}

fn manifest_response(manifest: Manifest) -> HttpResponse {
	let mut response = HttpResponse::Ok();
	response.insert_header((http::header::CONTENT_TYPE, manifest.media_type.to_string()));
	if let Some(digest) = manifest.digest {
		response.insert_header((HeaderName::from_static("docker-content-digest"), digest));
	}
	response.body(manifest.manifest)
}

pub async fn manifest(req: web::Path<ManifestRequest>, qstr: web::Query<ManifestQueryString>, config: web::Data<RequestConfig>) -> Result<HttpResponse, Error> {
	static HIT_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| register_int_counter_vec!("manifest_cache_hits", "Number of manifests read from cache", &["namespace"]).unwrap());
	static MISS_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| register_int_counter_vec!("manifest_cache_misses", "Number of manifest requests that went to upstream", &["namespace"]).unwrap());

	let (namespace, image) = split_image(qstr.ns.as_deref(), req.image.as_ref(), config.default_ns.as_ref());

	let max_age = config.upstream.lock().await.get(namespace.as_str())?.manifest_invalidation_time;
	let storage_path = req.storage_path(namespace.as_str());
	match config.repo.read(&storage_path, max_age).await {
		Ok(stream) => {
			let body = stream.into_inner().try_collect::<web::BytesMut>().await?;
			let manifest = serde_json::from_slice(body.as_ref())?;
			HIT_COUNTER.with_label_values(&[namespace.as_str()]).inc();
			return Ok(manifest_response(manifest));
		},
		Err(error) => warn!(path = req.http_path(), storage_path, %error, "Manifest not found in repository; pulling from upstream"),
	}

	MISS_COUNTER.with_label_values(&[namespace.as_str()]).inc();
	let manifest = {
		let mut upstream = config.upstream.lock().await.get(namespace.as_str())?.clone();
		authenticate_with_upstream(&mut upstream.client, &format!("repository:{}:pull", image)).await?;
		let reference = req.reference.to_str();
		let (manifest, media_type, digest) = match upstream.client.get_raw_manifest_and_metadata(image.as_str(), reference.as_ref(), Some(namespace.as_str())).await {
			Ok(v) => v,
			Err(e) if should_retry_without_namespace(&e) => upstream.client.get_raw_manifest_and_metadata(image.as_str(), reference.as_ref(), None).await?,
			Err(e) => return Err(e.into()),
		};
		Manifest::new(manifest, media_type, digest)
	};

	let body = serde_json::to_vec(&manifest).unwrap();
	let len = body.len().try_into().unwrap_or(i64::MAX);
	if let Err(error) = config
		.repo
		.write(&storage_path, futures::stream::iter(iter::once(Result::<_, std::io::Error>::Ok(body.into()))), len)
		.await
	{
		error!(%error, "Failed to write manifest to storage");
	}

	Ok(manifest_response(manifest))
}

#[derive(Debug, Deserialize)]
pub struct BlobRequest {
	image: ImageName,
	digest: String,
}

impl BlobRequest {
	fn http_path(&self) -> String {
		format!("/{}/blobs/{}", self.image, self.digest)
	}

	fn storage_path(&self) -> String {
		let (method, hash) = self.digest.split_once(':').unwrap_or(("_", &self.digest));
		let hash_prefix = hash.get(..2).unwrap_or("_");
		let rest_of_hash = hash.get(2..).unwrap_or(hash);
		format!("blobs/{method}/{hash_prefix}/{rest_of_hash}")
	}
}

pub async fn blob(req: web::Path<BlobRequest>, qstr: web::Query<ManifestQueryString>, config: web::Data<RequestConfig>) -> Result<HttpResponse, Error> {
	static HIT_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| register_int_counter_vec!("blob_cache_hits", "Number of blobs read from cache", &["namespace"]).unwrap());
	static MISS_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| register_int_counter_vec!("blob_cache_misses", "Number of blob requests that went to upstream", &["namespace"]).unwrap());

	let Some(wanted_digest_hex) = req.digest.strip_prefix("sha256:") else {
		return Err(Error::InvalidDigest);
	};
	let wanted_digest = {
		let mut buf = [0u8; 256 / 8];
		if (hex::decode_to_slice(wanted_digest_hex, &mut buf[..]).is_err()) {
			return Err(Error::InvalidDigest);
		}
		buf
	};

	let (namespace, image) = split_image(qstr.ns.as_deref(), req.image.as_ref(), config.default_ns.as_ref());

	let storage_path = req.storage_path();
	let max_age = config.upstream.lock().await.get(namespace.as_str())?.blob_invalidation_time;
	match config.repo.read(storage_path.as_ref(), max_age).await {
		Ok(stream) => match config.check_cache_digest {
			true => {
				let hash = stream::hash(stream.into_inner()).await?;
				if (hash == wanted_digest) {
					HIT_COUNTER.with_label_values(&[namespace.as_str()]).inc();
					let stream = config.repo.read(storage_path.as_ref(), max_age).await?;
					let len = stream.length();
					let (tx, rx) = async_broadcast::broadcast(16);
					if let Some(result) = chunk(req, &config, namespace, stream.into_inner().err_into::<crate::storage::Error>(), tx, true).await {
						return result;
					}
					return Ok(HttpResponse::Ok().body(SizedStream::new(len, rx.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))));
				}
				error!(storage_path, "Digest mismatch");
				config.repo.delete(storage_path.as_ref()).await?;
			},
			false => {
				HIT_COUNTER.with_label_values(&[namespace.as_str()]).inc();
				let stream = config.repo.read(storage_path.as_ref(), max_age).await?;
				let len = stream.length();
				let (tx, rx) = async_broadcast::broadcast(16);
				if let Some(result) = chunk(req, &config, namespace, stream.into_inner().err_into::<crate::storage::Error>(), tx, true).await {
					return result;
				}
				return Ok(HttpResponse::Ok().body(SizedStream::new(len, rx.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))));
			},
		},
		Err(error) => warn!(path = storage_path, %error, "Blob not found in repository; pulling from upstream"),
	};

	MISS_COUNTER.with_label_values(&[namespace.as_str()]).inc();
	let response = {
		let mut upstream = config.upstream.lock().await.get(namespace.as_str())?.clone();
		authenticate_with_upstream(&mut upstream.client, &format!("repository:{}:pull", image)).await?;
		match upstream.client.get_blob_response(image.as_str(), req.digest.as_ref(), Some(namespace.as_str())).await {
			Ok(v) => v,
			Err(e) if should_retry_without_namespace(&e) => upstream.client.get_blob_response(image.as_str(), req.digest.as_ref(), None).await?,
			Err(e) => return Err(e.into()),
		}
	};

	let len = response.size().ok_or(Error::MissingContentLength)?;
	let (tx, rx) = async_broadcast::broadcast(16);
	{
		let stream = DigestCheckedStream::<_, crate::storage::Error, _>::new(response.stream().err_into::<crate::storage::Error>(), wanted_digest);
		if let Some(result) = chunk(req, &config, namespace, stream, tx, false).await {
			return result;
		}
	}

	{
		let rx2 = rx.clone();
		let config = config.clone();
		rt::spawn(async move {
			if let Err(error) = config.repo.write(storage_path.as_ref(), rx2, len.try_into().unwrap_or(i64::MAX)).await {
				error!(%error, "Failed to write blob to storage");
				if let Err(error) = config.repo.delete(storage_path.as_ref()).await {
					error!(%error, "Failed to delete failed blob from storage");
				}
			}
		});
	}

	Ok(HttpResponse::Ok().body(SizedStream::new(len, rx.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))))
}

async fn chunk(
	req: web::Path<BlobRequest>,
	config: &web::Data<RequestConfig>,
	namespace: String,
	mut stream: impl Stream<Item = Result<web::Bytes, crate::storage::Error>> + Unpin + 'static,
	tx: async_broadcast::Sender<Result<web::Bytes, crate::storage::Error>>,
	hit: bool,
) -> Option<Result<HttpResponse, Error>> {
	static CHUNK_READ_DURATION_HISTOGRAM: Lazy<HistogramVec> = Lazy::new(|| register_histogram_vec!("blob_chunk_read_duration_seconds", "Duration of blob chunk reads", &["namespace", "hit"]).unwrap());
	static FIRST_CHUNK_READ_TIMEOUT_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| register_int_counter_vec!("blob_first_chunk_read_timeouts", "Number of blob chunk reads that timed out", &["namespace", "hit"]).unwrap());
	static CHUNK_WRITE_DURATION_HISTOGRAM: Lazy<HistogramVec> = Lazy::new(|| register_histogram_vec!("blob_chunk_write_duration_seconds", "Duration of blob chunk writes", &["namespace", "hit"]).unwrap());
	static FIRST_CHUNK_WRITE_TIMEOUT_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| register_int_counter_vec!("blob_first_chunk_write_timeouts", "Number of blob chunk writes that timed out", &["namespace", "hit"]).unwrap());

	// Read/write the first blob chunk with the provided timeouts
	while let Some(chunk) = {
		let chunk_read_timer = CHUNK_READ_DURATION_HISTOGRAM.with_label_values(&[namespace.as_str(), &hit.to_string()]).start_timer();
		let first_chunk_read_result = timeout(config.blob_first_chunk_read_timeout, stream.next()).await;
		chunk_read_timer.observe_duration();

		match first_chunk_read_result {
			Ok(chunk) => chunk,
			Err(_) => {
				FIRST_CHUNK_READ_TIMEOUT_COUNTER.with_label_values(&[namespace.as_str(), &hit.to_string()]).inc();
				error!(path = req.http_path(), "Timeout while reading first blob chunk");
				return Some(Err(Error::FirstChunkReadTimeout));
			},
		}
	} {
		let chunk = match chunk {
			Ok(v) => Ok(v),
			Err(error) => {
				error!(path = req.http_path(), %error, "Error reading from upstream");
				Err(error)
			},
		};

		let chunk_write_timer = CHUNK_WRITE_DURATION_HISTOGRAM.with_label_values(&[namespace.as_str(), &hit.to_string()]).start_timer();
		let first_chunk_write_result = timeout(config.blob_first_chunk_write_timeout, tx.broadcast(chunk)).await;
		chunk_write_timer.observe_duration();

		match first_chunk_write_result {
			Ok(Ok(_)) => (),
			Ok(Err(_)) => {
				error!(path = req.http_path(), "Readers for proxied blob request all closed");
				return Some(Err(Error::ReadersClosed));
			},
			Err(_) => {
				FIRST_CHUNK_WRITE_TIMEOUT_COUNTER.with_label_values(&[namespace.as_str(), &hit.to_string()]).inc();
				error!(path = req.http_path(), "Timeout while writing first blob chunk");
				return Some(Err(Error::FirstChunkWriteTimeout));
			},
		}

		break;
	}

	// Read/write the rest of the blob chunks
	rt::spawn(async move {
		Some(
			while let Some(chunk) = {
				let chunk_read_timer = CHUNK_READ_DURATION_HISTOGRAM.with_label_values(&[namespace.as_str(), &hit.to_string()]).start_timer();
				let chunk = stream.next().await;
				chunk_read_timer.observe_duration();
				chunk
			} {
				let chunk = match chunk {
					Ok(v) => Ok(v),
					Err(error) => {
						error!(%error, "Error reading from upstream");
						Err(error)
					},
				};

				let is_err = chunk.is_err();
				let chunk_write_timer = CHUNK_WRITE_DURATION_HISTOGRAM.with_label_values(&[namespace.as_str(), &hit.to_string()]).start_timer();
				if (tx.broadcast(chunk).await.is_err()) {
					error!(path = req.http_path(), "Readers for proxied blob request all closed");
					return Some(());
				} else if is_err {
					chunk_write_timer.observe_duration();
					return Some(());
				}
				chunk_write_timer.observe_duration();
			},
		)
	});

	None
}

#[inline]
pub fn split_image(ns: Option<&str>, image: &str, default_ns: &str) -> (String, String) {
	match ns {
		Some(v) => (v.to_string(), image.to_string()),
		None => match image.split_once('/') {
			Some((ns, image)) if image.contains('/') => (ns.to_string(), image.to_string()),
			Some(_) | None => (default_ns.to_string(), image.to_string()),
		},
	}
}

pub async fn delete_manifest(req: web::Path<ManifestRequest>, qstr: web::Query<ManifestQueryString>, config: web::Data<RequestConfig>) -> Result<&'static str, Error> {
	let (namespace, _) = split_image(qstr.ns.as_deref(), req.image.as_ref(), config.default_ns.as_ref());
	let storage_path = req.storage_path(namespace.as_str());
	config.repo.delete(storage_path.as_ref()).await?;
	Ok("")
}

pub async fn delete_blob(req: web::Path<BlobRequest>, config: web::Data<RequestConfig>) -> Result<&'static str, Error> {
	let storage_path = req.storage_path();
	config.repo.delete(storage_path.as_ref()).await?;
	Ok("")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn split_image_with_ns() {
		let (ns, image) = split_image(Some("docker.io"), "envoyproxy/envoy", "");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "envoyproxy/envoy");

		let (ns, image) = split_image(Some("docker.io"), "library/busybox", "");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "library/busybox");

		let (ns, image) = split_image(Some("docker.io"), "grafana/mimirtool", "");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "grafana/mimirtool");

		let (ns, image) = split_image(Some("gcr.io"), "distroless/static", "");
		assert_eq!(ns, "gcr.io");
		assert_eq!(image, "distroless/static");

		let (ns, image) = split_image(Some("gcr.io"), "flame-public/buildbuddy-app-onprem", "");
		assert_eq!(ns, "gcr.io");
		assert_eq!(image, "flame-public/buildbuddy-app-onprem");

		let (ns, image) = split_image(Some("ghcr.io"), "buildbarn/bb-runner-installer", "");
		assert_eq!(ns, "ghcr.io");
		assert_eq!(image, "buildbarn/bb-runner-installer");
	}

	#[test]
	fn split_image_without_ns() {
		let (ns, image) = split_image(None, "docker.io/envoyproxy/envoy", "");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "envoyproxy/envoy");

		let (ns, image) = split_image(None, "docker.io/library/busybox", "");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "library/busybox");

		let (ns, image) = split_image(None, "docker.io/grafana/mimirtool", "");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "grafana/mimirtool");

		let (ns, image) = split_image(None, "gcr.io/distroless/static", "");
		assert_eq!(ns, "gcr.io");
		assert_eq!(image, "distroless/static");

		let (ns, image) = split_image(None, "gcr.io/flame-public/buildbuddy-app-onprem", "");
		assert_eq!(ns, "gcr.io");
		assert_eq!(image, "flame-public/buildbuddy-app-onprem");

		let (ns, image) = split_image(None, "ghcr.io/buildbarn/bb-runner-installer", "");
		assert_eq!(ns, "ghcr.io");
		assert_eq!(image, "buildbarn/bb-runner-installer");
	}

	#[test]
	fn split_image_without_ns_fallback() {
		let (ns, image) = split_image(None, "envoyproxy/envoy", "docker.io");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "envoyproxy/envoy");

		let (ns, image) = split_image(None, "library/busybox", "docker.io");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "library/busybox");

		let (ns, image) = split_image(None, "grafana/mimirtool", "docker.io");
		assert_eq!(ns, "docker.io");
		assert_eq!(image, "grafana/mimirtool");
	}
}
