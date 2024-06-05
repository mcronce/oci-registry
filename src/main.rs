#![allow(unused_parens)]
use core::future;
use core::time::Duration;
use std::time::SystemTime;

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
	/// If enabled, will validate a blob's SHA256 digest when reading it from cache storage; if the
	/// digest doesn't match what was expected based on the request URL, it will be deleted from
	/// storage and re-retrieved from upstream.  This has an impact on performance, as the entire
	/// blob needs to be read from storage twice instead of just once.
	#[clap(env, long, default_value_t = false)]
	check_cache_digest: bool,
	#[clap(env, long, default_value = "30s")]
	blob_chunk_read_timeout: Duration,
	#[clap(env, long, default_value = "30s")]
	blob_chunk_write_timeout: Duration,
	#[clap(flatten)]
	upstream: UpstreamConfig,
	#[clap(subcommand)]
	storage: StorageConfig,
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
	let now = SystemTime::now();
	let mut count = match repo.delete_old_blobs(now - upstream.blob).await {
		Ok(v) => v,
		Err(error) => {
			error!(%error, "Error cleaning up blobs");
			0
		},
	};
	for (ns, age) in upstream.manifests.iter() {
		let ns: &str = ns.as_ref();
		match repo.delete_old_manifests(ns, now - *age).await {
			Ok(v) => count += v,
			Err(error) => error!(%error, namespace = ns, "Error cleaning up manifests"),
		};
	}

	if (count > 0) {
		warn!(count, "Aged out objects");
	} else {
		info!(count, "Aged out objects");
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

	let prometheus = PrometheusMetricsBuilder::new("http")
		.endpoint("/metrics")
		.buckets(&[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 20.0, 30.0, 60.0, 120.0, 180.0, 240.0, 300.0])
		.build()
		.unwrap();
	let per_request_config = web::Data::new(api::RequestConfig::new(
		repo,
		upstream,
		config.default_namespace,
		config.check_cache_digest,
		config.blob_chunk_read_timeout,
		config.blob_chunk_write_timeout,
	));

	let server = actix_web::HttpServer::new(move || {
		actix_web::App::new()
			.app_data(per_request_config.clone())
			.wrap(prometheus.clone())
			.service(
				web::scope("/v2")
					.wrap(actix_web::middleware::Logger::default())
					.route("/", web::get().to(api::root))
					// /v2/library/telegraf/manifests/1.24-alpine
					// /v2/library/redis/manifests/sha256:226cbafc637cd58cf008bf87ec9d1548ad1b672ef4279433495bdff100cdb883
					// /v2/docker.io/library/telegraf/manifests/1.24-alpine
					// /v2/docker.io/library/redis/manifests/sha256:226cbafc637cd58cf008bf87ec9d1548ad1b672ef4279433495bdff100cdb883
					.route("/{image:[^{}]+}/manifests/{reference}", web::head().to(api::manifest))
					.route("/{image:[^{}]+}/manifests/{reference}", web::get().to(api::manifest))
					// /v2/grafana/grafana/blobs/sha256:6864e61916f58174557076c34e7122753331cf28077edb0f23e1fb5419dd6acd
					// /v2/docker.io/grafana/grafana/blobs/sha256:6864e61916f58174557076c34e7122753331cf28077edb0f23e1fb5419dd6acd
					.route("/{image:[^{}]+}/blobs/{digest}", web::get().to(api::blob))
					.wrap_fn(|req, srv| {
						srv.call(req).map(|response| {
							response.map(|mut ok| {
								ok.headers_mut()
									.insert(HeaderName::from_static("docker-distribution-api-version"), HeaderValue::from_static("registry/2.0"));
								ok
							})
						})
					}),
			)
			.service(
				web::scope("/_admin")
					.wrap(actix_web::middleware::Logger::default())
					.route("/{image:[^{}]+}/manifests/{reference}", web::delete().to(api::delete_manifest))
					.route("/{image:[^{}]+}/blobs/{digest}", web::delete().to(api::delete_blob)),
			)
			.route("/", web::get().to(liveness))
	});
	match config.listen {
		socket_address::Address::Network(addr) => server.shutdown_timeout(10).bind(&addr).unwrap().run().await.unwrap(),
		socket_address::Address::UnixSocket(path) => server.shutdown_timeout(10).bind_uds(&path).unwrap().run().await.unwrap(),
	};
	shutdown_tx.send(()).unwrap();
	background.await.unwrap();
}
