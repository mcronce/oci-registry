#![allow(unused_parens)]
use core::future;
use core::time::Duration;

use actix_web::dev::Service;
use actix_web::http::header::HeaderName;
use actix_web::http::header::HeaderValue;
use actix_web::web;
use actix_web::HttpResponse;
use actix_web_prometheus::PrometheusMetricsBuilder;
use clap::Parser;
use compact_str::CompactString;
use futures::future::FutureExt;
use tokio::sync::oneshot;
use tracing::error;
use tracing::info;
use tracing::warn;

mod api;
mod image;
mod storage;
mod upstream;
mod util;

use storage::StorageConfig;
use upstream::InvalidationConfig;
use upstream::UpstreamConfig;

#[derive(Debug, Parser)]
struct Config {
	/// An IP address and port combination to listen on a network socket, or a path prefixed with
	/// "unix:" to listen on a Unix domain socket
	#[clap(env, long, default_value = "0.0.0.0:80")]
	listen: socket_address::Address,
	#[clap(env, long, default_value = "docker.io")]
	default_namespace: CompactString,
	#[clap(flatten)]
	upstream: UpstreamConfig,
	#[clap(subcommand)]
	storage: StorageConfig
}

#[inline]
fn liveness() -> future::Ready<HttpResponse> {
	future::ready(HttpResponse::Ok().body(""))
}

#[allow(dead_code)] // TODO:  Implement
#[inline]
async fn readiness() -> Result<&'static str, api::error::Error> {
	// TODO:  Check upstream and storage
	Ok("")
}

async fn cleanup(upstream: &InvalidationConfig, repo: &storage::Repository) {
	let mut count = match repo.delete_old_blobs(upstream.blob).await {
		Ok(v) => v,
		Err(error) => {
			error!(?error, "Error cleaning up blobs");
			0
		}
	};
	for (ns, age) in upstream.manifests.iter() {
		let ns: &str = ns.as_ref();
		match repo.delete_old_manifests(ns, *age).await {
			Ok(v) => count += v,
			Err(error) => error!(?error, namespace = ns, "Error cleaning up manifests")
		};
	}

	if (count > 0) {
		warn!("Aged out {count} objects");
	} else {
		info!("Aged out {count} objects");
	}
}

#[actix_web::main]
async fn main() {
	let config = Config::parse();

	tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).compact().init();

	let repo = config.storage.repository();
	let upstream = config.upstream.clients().await.unwrap();
	let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
	let background = {
		let repo = repo.clone();
		let upstream = upstream.invalidation_config();
		tokio::task::spawn(async move {
			let mut interval = tokio::time::interval(Duration::from_secs(300));
			loop {
				tokio::select! {
					_ = interval.tick() => (),
					_ = &mut shutdown_rx => break
				};
				cleanup(&upstream, &repo).await;
			}
		})
	};

	let per_request_config = web::Data::new(api::RequestConfig::new(repo, upstream, config.default_namespace));

	let server = actix_web::HttpServer::new(move || {
		let prometheus = PrometheusMetricsBuilder::new("oci_registry").endpoint("/metrics").build().unwrap();

		actix_web::App::new()
			.app_data(per_request_config.clone())
			.wrap(prometheus)
			.service(
				web::scope("/v2")
					.wrap(actix_web::middleware::Logger::default())
					.route("/", web::get().to(api::root))
					// /v2/library/telegraf/manifests/1.24-alpine
					// /v2/library/redis/manifests/sha256:226cbafc637cd58cf008bf87ec9d1548ad1b672ef4279433495bdff100cdb883
					.route("/{image:[^{}]+}/manifests/{reference}", web::head().to(api::manifest))
					.route("/{image:[^{}]+}/manifests/{reference}", web::get().to(api::manifest))
					// /v2/grafana/grafana/blobs/sha256:6864e61916f58174557076c34e7122753331cf28077edb0f23e1fb5419dd6acd
					.route("/{image:[^{}]+}/blobs/{digest}", web::get().to(api::blob))
					.wrap_fn(|req, srv| {
						srv.call(req).map(|response| {
							response.map(|mut ok| {
								ok.headers_mut()
									.insert(HeaderName::from_static("docker-distribution-api-version"), HeaderValue::from_static("registry/2.0"));
								ok
							})
						})
					})
			)
			.route("/", web::get().to(liveness))
	});
	match config.listen {
		socket_address::Address::Network(addr) => server.shutdown_timeout(10).bind(&addr).unwrap().run().await.unwrap(),
		socket_address::Address::UnixSocket(path) => server.shutdown_timeout(10).bind_uds(&path).unwrap().run().await.unwrap()
	};
	shutdown_tx.send(()).unwrap();
	background.await.unwrap();
}
