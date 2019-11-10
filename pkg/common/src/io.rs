use crate::errors::*;

const BUF_SIZE: usize = 4096;

/// An asynchronously readable object. Works similarly to std::io::Read except
/// allows multiple readers to operate simultaneously on the object. The
/// internal implementation is responsible for ensuring that any necessary
/// locking is performed.
#[async_trait]
pub trait Readable: Send + Sync + Unpin + 'static {
	async fn read(&self, buf: &mut [u8]) -> Result<usize>;

	// TODO: Deduplicate for http::Body
	async fn read_to_end(&self, buf: &mut Vec<u8>) -> Result<()> {
		let mut i = buf.len();
		loop {
			buf.resize(i + BUF_SIZE, 0);

			let res = self.read(&mut buf[i..]).await;
			match res {
				Ok(n) => {
					i += n;
					if n == 0 {
						buf.resize(i, 0);
						return Ok(());
					}
				},
				Err(e) => {
					buf.resize(i, 0);
					return Err(e);
				}
			}
		}
	}

	async fn read_exact(&self, mut buf: &mut [u8]) -> Result<()> {
		while buf.len() > 0 {
			let n = self.read(buf).await?;
			if n == 0 {
				return Err("Underlying stream closed".into());
			}

			buf = &mut buf[n..];
		}

		Ok(())
	}
}

#[async_trait]
pub trait Writeable: Send + Sync + Unpin + 'static {
	async fn write(&self, buf: &[u8]) -> Result<usize>;

	async fn flush(&self) -> Result<()>;

	async fn write_all(&self, mut buf: &[u8]) -> Result<()> {
		while buf.len() > 0 {
			let n = self.write(buf).await?;
			if n == 0 {
				return Err("Underlying stream closed".into());
			}

			buf = &buf[n..];
		}

		Ok(())
	}
}

#[async_trait]
impl Readable for async_std::net::TcpStream {
	async fn read(&self, buf: &mut [u8]) -> Result<usize> {
		let mut r = self;
		let n = async_std::io::Read::read(&mut r, buf).await?;
		Ok(n)
	}
}

#[async_trait]
impl Writeable for async_std::net::TcpStream {
	async fn write(&self, buf: &[u8]) -> Result<usize> {
		let mut r = self;
		let n = async_std::io::Write::write(&mut r, buf).await?;
		Ok(n)
	}

	async fn flush(&self) -> Result<()> {
		let mut r = self;
		async_std::io::Write::flush(&mut r).await?;
		Ok(())
	}
}

pub trait ReadWriteable : Readable + Writeable {
	fn as_read(&self) -> &dyn Readable;
	fn as_write(&self) -> &dyn Writeable;
}

impl <T: Readable + Writeable> ReadWriteable for T {
	fn as_read(&self) -> &dyn Readable { self }
	fn as_write(&self) -> &dyn Writeable { self }
}