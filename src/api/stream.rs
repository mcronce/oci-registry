use core::fmt;
use core::pin::Pin;

use actix_web::web::Bytes;
use futures::stream::Stream;
use futures::task::Context;
use futures::task::Poll;
use sha2::Digest;
use sha2::Sha256;

use crate::storage::Error;

pub struct DigestCheckedStream<S>
where
	S: Stream<Item = Result<Bytes, Error>> + Unpin,
{
	inner: S,
	wanted_digest: [u8; 32],
	hasher: Option<Sha256>,
}

impl<S> Stream for DigestCheckedStream<S>
where
	S: Stream<Item = Result<Bytes, Error>> + Unpin,
{
	type Item = Result<Bytes, Error>;

	fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let Poll::Ready(chunk) = Pin::new(&mut self.inner).poll_next(ctx) else {
			return Poll::Pending;
		};
		let Some(chunk) = chunk else {
			// When the stream has ended, we have one last step - finalize the hash.
			//   * If self.hasher is None, we've already done that; either the caller has made a
			//     mistake or we've previously returned a DigestMismatchError.
			//   * If the hash matches self.wanted_digest, return None to signal the end of the
			//     stream.
			//   * If the hash does not match self.wanted_digest, return a DigestMismatchError.
			return match std::mem::take(&mut self.hasher) {
				None => Poll::Ready(None),
				Some(hasher) => {
					let result: [u8; 32] = hasher.finalize().into();
					match (result == self.wanted_digest) {
						true => Poll::Ready(None),
						false => {
							let error = DigestMismatchError{
								expected: self.wanted_digest,
								actual: result
							};
							Poll::Ready(Some(Err(error.into())))
						}
					}
				}
			};
		};
		let Ok(chunk) = chunk else {
			return Poll::Ready(Some(chunk.into()));
		};
		if let Some(h) = self.hasher.as_mut() {
			h.update(&chunk);
		}
		Poll::Ready(Some(Ok(chunk)))
	}
}

impl<S> DigestCheckedStream<S>
where
	S: Stream<Item = Result<Bytes, Error>> + Unpin,
{
	pub fn new(inner: S, wanted_digest: [u8; 32]) -> Self {
		Self{
			inner,
			wanted_digest,
			hasher: Some(Sha256::new())
		}
	}
}

#[derive(Debug, Clone)]
pub struct DigestMismatchError {
	expected: [u8; 32],
	actual: [u8; 32]
}

impl fmt::Display for DigestMismatchError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("Digest '")?;
		for byte in self.actual.iter() {
			write!(f, "{:x}", byte)?;
		}
		f.write_str("' did not match expected '")?;
		for byte in self.expected.iter() {
			write!(f, "{:x}", byte)?;
		}
		f.write_str("'")
	}
}

impl std::error::Error for DigestMismatchError {}

