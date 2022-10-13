use clap::Parser;
use dkregistry::v2::Client;

#[derive(Debug, Parser)]
pub struct UpstreamConfig {
	#[clap(env, long, default_value = "registry-1.docker.io")]
	upstream_host: String,
	#[clap(env, long, default_value_t = true)]
	upstream_tls: bool,
	#[clap(env = "UPSTREAM_INSECURE_TLS", long = "upstream-insecure-tls")]
	upstream_accept_invalid_certs: bool,
	#[clap(env, long)]
	upstream_user_agent: Option<String>,
	#[clap(env, long)]
	upstream_username: Option<String>,
	#[clap(env, long)]
	upstream_password: Option<String>
}

impl UpstreamConfig {
	pub fn client(&self) -> Result<Client, dkregistry::errors::Error> {
		Client::configure()
			.registry(&self.upstream_host)
			.insecure_registry(!self.upstream_tls)
			.accept_invalid_certs(self.upstream_accept_invalid_certs)
			.user_agent(self.upstream_user_agent.clone())
			.username(self.upstream_username.clone())
			.password(self.upstream_password.clone())
			.build()
	}
}

