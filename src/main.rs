#![allow(unused_parens)]
use actix_web::dev::Service;
use actix_web::http::header::HeaderName;
use actix_web::http::header::HeaderValue;
use actix_web::web;
use clap::Parser;
use futures::future::FutureExt;
use tokio::sync::Mutex;

mod api;
mod image;
mod storage;
mod upstream;

use storage::StorageConfig;
use upstream::UpstreamConfig;

#[derive(Debug, Parser)]
struct Config {
	#[clap(env, long, default_value_t = 80)]
	port: u16,
	#[clap(env, long, default_value = "docker.io")]
	default_namespace: String,
	#[clap(flatten)]
	upstream: UpstreamConfig,
	#[clap(subcommand)]
	storage: StorageConfig
}

async fn health() -> Result<&'static str, api::error::Error> {
	// TODO:  Check upstream and storage
	Ok("")
}

#[actix_web::main]
async fn main() {
	let config = Config::parse();

	tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).compact().init();

	let repo = web::Data::new(config.storage.repository());
	let upstream = web::Data::new(Mutex::new(config.upstream.clients().await.unwrap()));
	let default_namespace = web::Data::new(config.default_namespace);

	let server = actix_web::HttpServer::new(move || {
		actix_web::App::new()
			.app_data(repo.clone())
			.app_data(upstream.clone())
			.app_data(default_namespace.clone())
			.wrap(actix_web::middleware::Logger::default())
			.route("/health", web::get().to(health))
			.service(
				web::scope("/v2")
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
	});
	server.shutdown_timeout(10).bind(&format!("0.0.0.0:{}", config.port)).unwrap().run().await.unwrap();
}
