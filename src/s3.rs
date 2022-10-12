use std::fmt;
use std::str::FromStr;

use clap::Parser;
use s3handler::CredentialConfig;
use s3handler::Handler;
use s3handler::none_blocking::primitives::S3Pool;

#[derive(Clone, Copy, Debug)]
pub enum S3Type {
	Aws,
	Ceph
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid S3 type '{0}'; must be 'AWS' or 'Ceph'")]
pub struct InvalidS3Type(String);

impl FromStr for S3Type {
	type Err = InvalidS3Type;
	fn from_str(input: &str) -> Result<Self, Self::Err> {
		if(input.eq_ignore_ascii_case("aws")) {
			Ok(Self::Aws)
		} else if(input.eq_ignore_ascii_case("ceph")) {
			Ok(Self::Ceph)
		} else {
			Err(InvalidS3Type(input.to_string()))
		}
	}
}

impl fmt::Display for S3Type {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
		match *self {
			Self::Aws => write!(f, "aws"),
			Self::Ceph => write!(f, "ceph")
		}
	}
}

#[derive(Clone, Debug, Parser)]
#[group(skip)]
pub struct Config {
	#[clap(env, long)]
	s3_host: String,
	#[clap(env, long)]
	s3_access_key: String,
	#[clap(env, long)]
	s3_secret_key: String,
	#[clap(env, long, default_value = "us-east-1")]
	s3_region: String,
	#[clap(env, long, default_value_t = S3Type::Aws)]
	s3_type: S3Type,
	#[clap(env, long)]
	s3_secure: bool
}

impl Config {
	pub fn client(&self) -> S3Pool {
		let config = CredentialConfig{
			host: self.s3_host.clone(),
			user: None,
			access_key: self.s3_access_key.clone(),
			secret_key: self.s3_secret_key.clone(),
			region: Some(self.s3_region.clone()),
			s3_type: Some(self.s3_type.to_string()),
			secure: Some(self.s3_secure)
		};
		let handler = Handler::from(&config);
		handler.into()
	}
}

