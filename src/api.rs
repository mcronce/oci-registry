use core::time::Duration;
use std::iter;

use actix_web::http;
use actix_web::rt;
use actix_web::web;
use actix_web::HttpResponse;
use arcerror::ArcError;
use arcstr::ArcStr;
use dkregistry::v2::Client;
use futures::stream;
use futures::StreamExt;
use futures::TryStreamExt;
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
use error::Error;

async fn authenticate_with_upstream(upstream: &mut Client, scope: &str) -> Result<(), dkregistry::errors::Error> {
	upstream.authenticate(&[scope]).await?;
	Ok(())
}

pub async fn root(upstream: web::Data<Mutex<Clients>>, qstr: web::Query<ManifestQueryString>) -> Result<&'static str, Error> {
	upstream.lock().await.get(qstr.ns.as_deref())?.client.authenticate(&[]).await?;
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
		format!("manifests/{}/{}/{}", ns, self.image, self.reference)
	}
}

#[derive(Debug, Deserialize)]
pub struct ManifestQueryString {
	ns: Option<String>
}

async fn get_manifest(req: &ManifestRequest, max_age: Duration, repo: &Repository, upstream: &mut Client, namespace: &str) -> Result<Manifest, Error> {
	let storage_path = req.storage_path(namespace);
	match repo.clone().read(&storage_path, max_age).await {
		Ok(stream) => {
			let body = stream.try_collect::<web::BytesMut>().await?;
			let manifest = serde_json::from_slice(body.as_ref())?;
			return Ok(manifest);
		},
		Err(e) => warn!("{} not found at {} in repository ({}); pulling from upstream", req.http_path(), storage_path, e)
	}

	authenticate_with_upstream(upstream, &format!("repository:{}:pull", req.image.as_ref())).await?;
	let (manifest, media_type, digest) = upstream.get_raw_manifest_and_metadata(req.image.as_ref(), &req.reference.to_string()).await?;
	let manifest = Manifest::new(manifest, media_type, digest);

	let body = serde_json::to_vec(&manifest).unwrap();
	let len = body.len().try_into().unwrap_or(i64::MAX);
	if let Err(e) = repo.write(&storage_path, stream::iter(iter::once(Result::<_, std::io::Error>::Ok(body.into()))), len).await {
		error!("{}", e);
	}
	Ok(manifest)
}

pub async fn manifest(req: web::Path<ManifestRequest>, qstr: web::Query<ManifestQueryString>, repo: web::Data<Repository>, upstream: web::Data<Mutex<Clients>>, default_ns: web::Data<String>) -> Result<HttpResponse, Error> {
	let mut upstream = upstream.lock().await.get(qstr.ns.as_deref())?;
	let manifest = get_manifest(req.as_ref(), upstream.manifest_invalidation_time, repo.as_ref(), &mut upstream.client, qstr.ns.as_ref().unwrap_or_else(|| default_ns.as_ref())).await?;

	let mut response = HttpResponse::Ok();
	response.insert_header((http::header::CONTENT_TYPE, manifest.media_type.to_string()));
	if let Some(digest) = manifest.digest.clone() {
		response.insert_header(("Docker-Content-Digest", digest));
	}
	Ok(response.body(manifest.manifest))
}

#[derive(Debug, Deserialize)]
pub struct BlobRequest {
	image: ImageName,
	digest: String
}

impl BlobRequest {
	fn http_path(&self) -> ArcStr {
		format!("/{}/blobs/{}", self.image, self.digest).into()
	}

	fn storage_path(&self) -> ArcStr {
		let (method, hash) = self.digest.split_once(':').unwrap_or(("_", &self.digest));
		let hash_prefix = hash.get(..2).unwrap_or("_");
		let rest_of_hash = hash.get(2..).unwrap_or(hash);
		format!("blobs/{}/{}/{}", method, hash_prefix, rest_of_hash).into()
	}
}

pub async fn blob(req: web::Path<BlobRequest>, qstr: web::Query<ManifestQueryString>, repo: web::Data<Repository>, upstream: web::Data<Mutex<Clients>>) -> Result<HttpResponse, Error> {
	if(!req.digest.starts_with("sha256:")) {
		return Err(Error::InvalidDigest);
	}

	let req_path = req.http_path();
	let storage_path = req.storage_path();
	let max_age = upstream.lock().await.get(qstr.ns.as_deref())?.blob_invalidation_time;
	match (*repo.clone().into_inner()).clone().read(storage_path.as_ref(), max_age).await {
		Ok(stream) => return Ok(HttpResponse::Ok().streaming(stream)),
		Err(e) => warn!("{} not found in repository ({}); pulling from upstream", storage_path, e)
	};

	let mut upstream = upstream.lock().await.get(qstr.ns.as_deref())?;
	authenticate_with_upstream(&mut upstream.client, &format!("repository:{}:pull", req.image.as_ref())).await?;
	let response = upstream.client.get_blob_response(req.image.as_ref(), req.digest.as_ref()).await?;

	let len = response.size().unwrap_or_default();
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
				if(tx.broadcast(chunk).await.is_err()) {
					error!("Readers for proxied blob request {} all closed", req_path);
					break;
				}
			}
		});
	}

	let rx2 = rx.clone();
	rt::spawn(async move {
		if let Err(e) = repo.write(storage_path.as_ref(), rx2, len.try_into().unwrap_or(i64::MAX)).await {
			error!("{}", e);
		}
	});

	Ok(HttpResponse::Ok().streaming(rx))
}

