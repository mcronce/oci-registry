use core::convert::Infallible;
use core::fmt;
use core::str::FromStr;

use serde::Deserialize;
use serde::Deserializer;

#[derive(Clone)]
pub(crate) struct SecretString(String);
impl FromStr for SecretString {
	type Err = Infallible;

	#[inline]
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(s.into())
	}
}

impl From<&str> for SecretString {
	#[inline]
	fn from(s: &str) -> Self {
		Self(s.into())
	}
}

impl fmt::Debug for SecretString {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("\"REDACTED\"")
	}
}

impl<'de> Deserialize<'de> for SecretString {
	#[inline]
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let s = String::deserialize(deserializer)?;
		Ok(Self(s))
	}
}

impl SecretString {
	#[inline]
	pub(crate) fn into_inner(self) -> String {
		self.0
	}
}
