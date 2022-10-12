#![allow(unused_parens)]
use actix_web::dev::Service;
use actix_web::http::header::HeaderName;
use actix_web::http::header::HeaderValue;
use actix_web::web;
use clap::Parser;

mod api;
mod image;
mod s3;
mod upstream;

#[derive(Debug, Parser)]
struct Config {
	#[clap(env, long, default_value_t = 80)]
	port: u16,
	#[clap(flatten)]
	s3: s3::Config,
	#[clap(flatten)]
	upstream: upstream::Config,
}

#[actix_web::main]
async fn main() {
	let config = Config::parse();

	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.compact()
		.init();

	let s3 = web::Data::new(config.s3.client());
	let upstream = web::Data::new(config.upstream.client().unwrap());

	let server = actix_web::HttpServer::new(move || actix_web::App::new()
		.app_data(s3.clone())
		.app_data(upstream.clone())
		.wrap(actix_web::middleware::Logger::default())
		.service(web::scope("/v2")
			.route("/", web::get().to(api::root))
			// /v2/library/telegraf/manifests/1.24-alpine
			.route("/{image:[^{}]+}/manifests/{reference}", web::head().to(api::check_manifest))
			// /v2/library/redis/manifests/sha256:226cbafc637cd58cf008bf87ec9d1548ad1b672ef4279433495bdff100cdb883
			.route("/{image:[^{}]+}/manifests/{reference}", web::get().to(api::manifest))
			// /v2/grafana/grafana/blobs/sha256:6864e61916f58174557076c34e7122753331cf28077edb0f23e1fb5419dd6acd
			.route("/{image:[^{}]+}/blobs/{digest}", web::get().to(api::blob))
			.wrap_fn(|req, srv| {
				let fut = srv.call(req);
				async {
					let mut response = fut.await?;
					response.headers_mut().insert(HeaderName::from_static("docker-distribution-api-version"), HeaderValue::from_static("registry/2.0"));
					Ok(response)
				}
			})
		)
	);
	server
		.shutdown_timeout(10)
		.bind(&format!("0.0.0.0:{}", config.port))
		.unwrap()
		.run()
		.await
		.unwrap();
}

