use core::time::Duration;
use std::path::Path;
use std::time::SystemTime;

use actix_web::web::Bytes;
use async_stream::try_stream;
use async_walkdir::WalkDir;
use camino::Utf8Component;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap::Parser;
use futures::stream::StreamExt;
use futures::stream::TryStream;
use futures::stream::TryStreamExt;
use tokio::fs::create_dir_all;
use tokio::fs::remove_file;
use tokio::fs::symlink_metadata;
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;
use tracing::error;
use tracing::info;

use super::ReadStream;

#[derive(Clone, Debug, Parser)]
pub struct Config {
	#[clap(env = "FILESYSTEM_ROOT", long)]
	root: Utf8PathBuf
}

impl Config {
	pub fn repository(&self) -> Repository {
		Repository { root: self.root.clone() }
	}
}

#[derive(Debug, Clone)]
pub struct Repository {
	root: Utf8PathBuf
}

impl Repository {
	fn full_path(&self, path: &Utf8Path) -> Utf8PathBuf {
		let path = path.components().filter(|c| matches!(c, Utf8Component::ParentDir | Utf8Component::Normal(_))).collect::<Utf8PathBuf>();
		self.root.join(path)
	}

	pub async fn read(&self, object: &Utf8Path, invalidation: Duration) -> Result<ReadStream, super::Error> {
		let path = self.full_path(object);
		let (age, length) = {
			let metadata = symlink_metadata(&path).await?;
			(SystemTime::now().duration_since(metadata.modified()?).unwrap_or_default(), metadata.len())
		};
		if (age > invalidation) {
			return Err(super::Error::ObjectTooOld(age.into()));
		}
		let mut file = BufReader::with_capacity(16384, File::open(path).await?);
		Ok(ReadStream::new(
			length,
			Box::pin(try_stream! {
				loop {
					let buf = file.fill_buf().await?;
					if(buf.is_empty()) {
						break;
					}
					let len = buf.len();
					yield Bytes::copy_from_slice(buf);
					file.consume(len);
				}
			})
		))
	}

	pub async fn write<S, E>(&self, object: &Utf8Path, reader: S) -> Result<(), super::Error>
	where
		S: TryStream<Ok = Bytes, Error = E> + Unpin,
		E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
		super::Error: From<E>
	{
		async fn _write<S, E>(file: &mut BufWriter<File>, mut reader: S) -> Result<(), super::Error>
		where
			S: TryStream<Ok = Bytes, Error = E> + Unpin,
			E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
			super::Error: From<E>
		{
			while let Some(buf) = reader.try_next().await? {
				if (buf.is_empty()) {
					break;
				}
				file.write_all(buf.as_ref()).await.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
			}
			Ok(())
		}

		let path = self.full_path(object);
		if let Some(parent) = path.parent() {
			create_dir_all(parent).await?;
		}
		let file = OpenOptions::default().create(true).read(false).write(true).truncate(true).open(&path).await?;
		let mut file = BufWriter::with_capacity(16384, file);

		match _write(&mut file, reader).await {
			Ok(_) => Ok(file.flush().await?),
			Err(e) => {
				self.delete(object.as_ref()).await?;
				Err(e)
			}
		}
	}

	pub async fn delete(&self, path: &Path) -> Result<(), std::io::Error> {
		remove_file(path).await
	}

	pub async fn delete_old_files(&self, older_than: SystemTime, prefix: &Utf8Path) -> Result<usize, super::Error> {
		let mut count = 0;
		let root = self.root.join(prefix);
		let mut entries = WalkDir::new(root);
		let mut first_iteration = true;
		while let Some(entry) = entries.next().await {
			let entry = match entry {
				Ok(v) => v,
				// If we get a NotFound error on the first iteration, it only means that we haven't cached anything under this prefix yet
				Err(e) if e.kind() == std::io::ErrorKind::NotFound && first_iteration => continue,
				Err(error) => {
					error!(path = %prefix, %error, "Error walking directory");
					continue;
				}
			};
			first_iteration = false;
			let path = entry.path();
			let metadata = match entry.metadata().await {
				Ok(v) => v,
				Err(error) => {
					error!(path = %path.display(), %error, "Error reading metadata");
					continue;
				}
			};
			if (!metadata.is_file()) {
				continue;
			}
			let modified = match metadata.modified() {
				Ok(v) => v,
				Err(error) => {
					error!(path = %path.display(), %error, "Error reading mtime");
					continue;
				}
			};
			if (modified < older_than) {
				match self.delete(&path).await {
					Ok(_) => info!(path = %path.display(), "Aged out"),
					Err(error) => {
						error!(path = %path.display(), %error, "Error deleting object");
						continue;
					}
				}
				count += 1;
			}
		}
		Ok(count)
	}
}
