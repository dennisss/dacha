use common::errors::*;
use async_std::fs::File;
use async_std::io::Read;

/// Generates secure random bytes suitable for cryptographic key generation.
/// This will wait for sufficient entropy to accumulate in the system.
/// 
/// Once done, the provided buffer will be filled with the random bytes to the
/// end.
async fn secure_random_bytes(buf: &mut [u8]) -> Result<()> {
	// See http://man7.org/linux/man-pages/man7/random.7.html
	// TODO: Reuse the file handle across calls.
	let mut f = File::open("/dev/random").await?;
	f.read_exact(buf).await?;
	Ok(())
}