use common::errors::*;
use async_std::fs::File;
use async_std::io::Read;
use crate::big_number::BigUint;

/// Generates secure random bytes suitable for cryptographic key generation.
/// This will wait for sufficient entropy to accumulate in the system.
/// 
/// Once done, the provided buffer will be filled with the random bytes to the
/// end.
pub async fn secure_random_bytes(buf: &mut [u8]) -> Result<()> {
	// See http://man7.org/linux/man-pages/man7/random.7.html
	// TODO: Reuse the file handle across calls.
	let mut f = File::open("/dev/random").await?;
	f.read_exact(buf).await?;
	Ok(())
}


/// Securely generates a random value in the range '[lower, upper)'.
/// This is implemented to give every integer in the range the same probabiity
/// of being output.
pub async fn secure_random_range(lower: &BigUint, upper: &BigUint)
-> Result<BigUint> {
	if upper.min_bytes() == 0 || upper <= lower {
		return Err("Invalid upper/lower range".into());
	}

	let mut buf = vec![];
	buf.resize(upper.min_bytes(), 0);

	let msb_mask: u8 = {
		let r = upper.nbits() % 8;
		!((1 << (8 - r)) - 1)
	};

	loop {
		secure_random_bytes(&mut buf).await?;
		*buf.last_mut().unwrap() &= msb_mask;

		let n = BigUint::from_le_bytes(&buf);
		
		// TODO: This *must* be a secure comparison (which it isn't right now).
		if &n >= lower && &n < upper {
			return Ok(n);
		}
	}
}