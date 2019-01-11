pub mod stream;
pub mod superblock;
mod machine_index;
pub mod machine;
pub mod needle;
mod volume_index;
pub mod volume;
pub mod routes;
pub mod main;


use super::common::BLOCK_SIZE;

/// Given that the current position in the file is at the end of a middle, this will determine how much 
fn block_size_remainder(end_offset: u64) -> u64 {
	let rem = (end_offset as usize) % BLOCK_SIZE;
	if rem == 0 {
		return 0;
	}

	(BLOCK_SIZE - rem) as u64
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn block_size_remainder_test() {
		let bsize = BLOCK_SIZE as u64;
		assert_eq!(block_size_remainder(0), 0);
		assert_eq!(block_size_remainder(3*bsize), 0);
		assert_eq!(block_size_remainder(bsize - 4), 4);
		assert_eq!(block_size_remainder(6*bsize + 5), bsize - 5);
	}

}
