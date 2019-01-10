pub mod stream;
pub mod superblock;
mod machine_index;
pub mod machine;
pub mod needle;
mod volume_index;
pub mod volume;
pub mod routes;


use super::common::BLOCK_SIZE;

/// Given that the current position in the file is at the end of a middle, this will determine how much 
fn block_size_remainder(end_offset: u64) -> u64 {
	let rem = (end_offset as usize) % BLOCK_SIZE;
	if rem == 0 {
		return 0;
	}

	(BLOCK_SIZE - rem) as u64
}