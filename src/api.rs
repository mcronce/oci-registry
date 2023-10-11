use std::iter;

use actix_web::body::SizedStream;
use actix_web::http;
use actix_web::http::header::HeaderName;
use actix_web::rt;
use actix_web::web;
use actix_web::HttpResponse;
use arcerror::ArcError;
use compact_str::CompactString;
use dkregistry::v2::Client;
use futures::stream;
use futures::StreamExt;
use futures::TryStreamExt;
use once_cell::sync::Lazy;
use prometheus::IntCounterVec;
use prometheus::register_int_counter_vec;
use serde::Deserialize;
use tokio::sync::Mutex;
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

pub struct RequestConfig {
	repo: Repository,
	upstream: Mutex<Clients>,
	default_ns: CompactString
}

impl RequestConfig {
	pub fn new(repo: Repository, upstream: Clients, default_ns: CompactString) -> Self {
		Self { repo, upstream: Mutex::new(upstream), default_ns }
	}
}

async fn authenticate_with_upstream(upstream: &mut Client, scope: &str) -> Result<(), dkregistry::errors::Error> {
	upstream.authenticate(&[scope]).await?;
	Ok(())
}

pub async fn root(config: web::Data<RequestConfig>, qstr: web::Query<ManifestQueryString>) -> Result<&'static str, Error> {
	config.upstream.lock().await.get(qstr.ns.as_deref().unwrap_or_else(|| config.default_ns.as_ref()))?.client.authenticate(&[]).await?;
	Ok("")
}

#[derive(Debug, Deserialize)]
pub struct ManifestRequest {
	image: ImageName,
	reference: ImageReference
}

impl ManifestRequest {
	fn http_path(&self) -> String {
		format!("/{}/manifests/{}", self.image, self.reference)
	}

	fn storage_path(&self, ns: &str) -> String {
		if ns == self.image.as_ref().split("/").next().unwrap_or_default() {
			return format!("manifests/{}/{}", self.image, self.reference);
		}
		format!("manifests/{}/{}/{}", ns, self.image, self.reference)
	}
}

#[derive(Debug, Deserialize)]
pub struct ManifestQueryString {
	ns: Option<CompactString>
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

	let mut namespace = qstr.ns.as_deref().unwrap_or_else(|| config.default_ns.as_ref());
	if req.image.as_ref().split("/").count() > 2 {
		namespace = req.image.as_ref().split("/").next().unwrap_or_else(|| config.default_ns.as_ref());
	}

	let image: &str = req.image.as_ref().trim_start_matches("docker.io/");
	let max_age = config.upstream.lock().await.get(namespace)?.manifest_invalidation_time;
	let storage_path = req.storage_path(namespace);
	match config.repo.read(&storage_path, max_age).await {
		Ok(stream) => {
			let body = stream.into_inner().try_collect::<web::BytesMut>().await?;
			let manifest = serde_json::from_slice(body.as_ref())?;
			HIT_COUNTER.with_label_values(&[namespace]).inc();
			return Ok(manifest_response(manifest));
		},
		Err(e) => warn!("{} not found at {} in repository ({}); pulling from upstream", req.http_path(), storage_path, e)
	}

	MISS_COUNTER.with_label_values(&[namespace]).inc();
	let manifest = {
		let mut upstream = config.upstream.lock().await;
		let upstream = upstream.get(namespace)?;
		authenticate_with_upstream(&mut upstream.client, &format!("repository:{}:pull", image)).await?;
		let reference = req.reference.to_str();
		let (manifest, media_type, digest) = match upstream.client.get_raw_manifest_and_metadata(image, reference.as_ref(), Some(namespace)).await {
			Ok(v) => v,
			Err(e) if should_retry_without_namespace(&e) => upstream.client.get_raw_manifest_and_metadata(image, reference.as_ref(), None).await?,
			Err(e) => return Err(e.into())
		};
		Manifest::new(manifest, media_type, digest)
	};

	let body = serde_json::to_vec(&manifest).unwrap();
	let len = body.len().try_into().unwrap_or(i64::MAX);
	if let Err(e) = config.repo.write(&storage_path, stream::iter(iter::once(Result::<_, std::io::Error>::Ok(body.into()))), len).await {
		error!("{}", e);
	}

	Ok(manifest_response(manifest))
}

#[derive(Debug, Deserialize)]
pub struct BlobRequest {
	image: ImageName,
	digest: String
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

	if (!req.digest.starts_with("sha256:")) {
		return Err(Error::InvalidDigest);
	}

	let mut namespace = qstr.ns.as_deref().unwrap_or_else(|| config.default_ns.as_ref());
	if req.image.as_ref().split("/").count() > 2 {
		namespace = req.image.as_ref().split("/").next().unwrap_or_else(|| config.default_ns.as_ref());
	}

	let image: &str = req.image.as_ref().trim_start_matches("docker.io/");
	let storage_path = req.storage_path();
	let max_age = config.upstream.lock().await.get(namespace)?.blob_invalidation_time;
	match config.repo.read(storage_path.as_ref(), max_age).await {
		Ok(stream) => {
			HIT_COUNTER.with_label_values(&[namespace]).inc();
			return Ok(HttpResponse::Ok().body(SizedStream::from(stream)));
		},
		Err(e) => warn!("{} not found in repository ({}); pulling from upstream", storage_path, e)
	};

	MISS_COUNTER.with_label_values(&[namespace]).inc();
	let response = {
		let mut upstream = config.upstream.lock().await;
		let upstream = upstream.get(namespace)?;
		authenticate_with_upstream(&mut upstream.client, &format!("repository:{}:pull", image)).await?;
		match upstream.client.get_blob_response(image, req.digest.as_ref(), Some(namespace)).await {
			Ok(v) => v,
			Err(e) if should_retry_without_namespace(&e) => upstream.client.get_blob_response(image, req.digest.as_ref(), None).await?,
			Err(e) => return Err(e.into())
		}
	};

	let len = response.size().ok_or(Error::MissingContentLength)?;
	let (tx, rx) = async_broadcast::broadcast(16);
	{
		let mut stream = response.stream();
		rt::spawn(async move {
			while let Some(chunk) = stream.next().await {
				let chunk = match chunk {
					Ok(v) => Ok(v),
					Err(e) => {
						error!("Error reading from upstream:  {}", e);
						Err(ArcError::from(e))
					}
				};
				if (tx.broadcast(chunk).await.is_err()) {
					error!("Readers for proxied blob request {} all closed", req.http_path());
					break;
				}
			}
		});
	}

	{
		let rx2 = rx.clone();
		let config = config.clone();
		rt::spawn(async move {
			if let Err(e) = config.repo.write(storage_path.as_ref(), rx2, len.try_into().unwrap_or(i64::MAX)).await {
				error!("{}", e);
			}
		});
	}

	Ok(HttpResponse::Ok().body(SizedStream::new(len, rx.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))))
}
