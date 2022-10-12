use std::sync::Arc;

use actix_web::http;
use actix_web::web;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use dkregistry::v2::manifest::Manifest;
use dkregistry::v2::Client;
use s3handler::none_blocking::primitives::S3Pool;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::image::ImageName;
use crate::image::ImageReference;

mod error;
use error::Error;

async fn authenticate_with_upstream(upstream: &mut Client, scope: &str) -> Result<(), dkregistry::errors::Error> {
	println!("{}", scope);
	upstream.authenticate(&[scope]).await?;
	Ok(())
}

pub async fn root(upstream: web::Data<Client>) -> Result<&'static str, Error> {
	Arc::make_mut(&mut upstream.into_inner())
		.clone()
		.authenticate(&[])
		.await?;
	Ok("")
}

#[derive(Debug, Deserialize)]
pub struct ManifestRequest {
	image: ImageName,
	reference: ImageReference
}

async fn get_manifest(req: &ManifestRequest, s3: &S3Pool, upstream: web::Data<Client>) -> Result<(Manifest, Option<String>), Error> {
	let mut upstream = (*upstream.into_inner()).clone();
	authenticate_with_upstream(&mut upstream, &format!("repository:{}:pull", req.image.as_ref())).await?;
	let (manifest, digest) = upstream.get_manifest_and_ref(req.image.as_ref(), &req.reference.to_string()).await?;
	Ok((manifest, digest))
}

pub async fn manifest(path: web::Path<ManifestRequest>, s3: web::Data<S3Pool>, upstream: web::Data<Client>) -> Result<HttpResponse, Error> {
	let (manifest, digest) = get_manifest(path.as_ref(), s3.as_ref(), upstream).await?;
	let media_type = manifest.media_type();
	let manifest = serde_json::to_string(&manifest).unwrap();

	let mut response = HttpResponse::Ok();
	response.insert_header((http::header::CONTENT_TYPE, media_type.to_string()));
	if let Some(digest) = digest {
		response.insert_header(("Docker-Content-Digest", digest));
	}
	Ok(response.body(manifest))
}

pub async fn check_manifest(path: web::Path<ManifestRequest>, s3: web::Data<S3Pool>, upstream: web::Data<Client>) -> Result<HttpResponse, Error> {
	let (manifest, digest) = get_manifest(path.as_ref(), s3.as_ref(), upstream).await?;
	let media_type = manifest.media_type();
	let manifest = serde_json::to_string(&manifest).unwrap();

	let mut response = HttpResponse::Ok();
	response.insert_header((http::header::CONTENT_TYPE, media_type.to_string()));
	response.insert_header((http::header::CONTENT_LENGTH, manifest.len()));
	if let Some(digest) = digest {
		response.insert_header(("Docker-Content-Digest", digest));
	}
	Ok(response.finish())
}

#[derive(Debug, Deserialize)]
pub struct BlobRequest {
	image: ImageName,
	digest: String
}

pub async fn blob(path: web::Path<BlobRequest>, s3: web::Data<S3Pool>, upstream: web::Data<Client>) -> Result<Vec<u8>, Error> {
	if(!path.digest.starts_with("sha256:")) {
		return Err(Error::InvalidDigest);
	}
	let mut upstream = (*upstream.into_inner()).clone();
	authenticate_with_upstream(&mut upstream, &format!("repository:{}:pull", path.image.as_ref())).await?;
	let blob = upstream.get_blob(path.image.as_ref(), path.digest.as_ref()).await?;
	Ok(blob)
}

