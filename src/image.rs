use std::fmt;
use std::str::FromStr;

use compact_str::CompactString;
use lazy_regex::lazy_regex;
use lazy_regex::Lazy;
use lazy_regex::Regex;
use serde_with::DeserializeFromStr;

mod error;

static RE_IMAGE: Lazy<Regex> = lazy_regex!("^[a-z0-9]+([._-][a-z0-9]+)*(/[a-z0-9]+([._-][a-z0-9]+)*)*$");
static RE_TAG: Lazy<Regex> = lazy_regex!("^[a-zA-Z0-9_][a-zA-Z0-9._-]{0,127}$");

fn is_valid_sha256(s: &str) -> bool {
	s.len() == 64 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) && hex::decode(s).is_ok()
}

#[derive(Debug, DeserializeFromStr)]
pub struct ImageName(CompactString);
impl FromStr for ImageName {
	type Err = error::InvalidImageName;

	fn from_str(input: &str) -> Result<Self, Self::Err> {
		match RE_IMAGE.is_match(input) {
			false => Err(error::InvalidImageName(input.to_string())),
			true => Ok(ImageName(input.into()))
		}
	}
}

impl AsRef<str> for ImageName {
	#[inline]
	fn as_ref(&self) -> &str {
		self.0.as_ref()
	}
}

impl fmt::Display for ImageName {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

#[derive(Debug, DeserializeFromStr)]
pub enum ImageReference {
	Tag(CompactString),
	Sha256(String)
}

impl FromStr for ImageReference {
	type Err = error::InvalidImageReference;

	fn from_str(input: &str) -> Result<Self, Self::Err> {
		match input.strip_prefix("sha256:") {
			None => match RE_TAG.is_match(input) {
				false => Err(error::InvalidImageReference(input.to_string())),
				true => Ok(ImageReference::Tag(input.into()))
			},
			Some(s) => match is_valid_sha256(s) {
				false => Err(error::InvalidImageReference(input.to_string())),
				true => Ok(ImageReference::Sha256(s.into()))
			}
		}
	}
}

impl fmt::Display for ImageReference {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Tag(s) => s.fmt(f),
			Self::Sha256(s) => write!(f, "sha256:{s}")
		}
	}
}
