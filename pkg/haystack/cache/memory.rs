use super::store::needle::*;
use std::collections::HashMap;
use std::collections::BTreeMap;
use std::time::*;


pub struct MemoryEntry {

	pub cookie: Cookie;

	/// The logical volume from which we got this entry from originally
	/// Because photos can change logical volumes upon overflow of their old ones, this may change over time
	/// TODO: Because we don't include this in the cache key, sequential attempts to lookup different volumes will cache miss a lot, so ideally we hope that all browsers quickly pick up the new volume id
	/// Additionally because the old volume will typically reside of a read-only machine, we will typically not end up recaching old versions
	pub logical_id: VolumeId,

	/// THis will be the raw data of the needle file it is associated with 
	pub data: Vec<u8>,
}


/// TODO: What is the best strategy for clearing out old copies
/// Currently we ass

struct Entry {

	pub data: MemoryEntry,

	/// When this entry was inserted
	pub inserted_at: SystemTime,

	/// The last time this data was 
	pub last_access: SystemTime,
	
	/// The time which we stored in the order tree
	pub last_order: SystemTime
}

/// A simple LRU in-memory cache with 
pub struct MemoryStore {

	/// Maximum amount of space we are allowed to take up in-memory
	total_space: usize,

	max_age: Duration,


	/// Amount of memory in bytes used up by all cache entries (excluding the metadata needed to store them)
	used_space: usize,

	/// TODO: The real question will be how to deal with 
	index: HashMap<NeedleKeys, CacheEntry>,

	// TODO: In case it switches volumes, 
	order: BTreeMap<SystemTime, NeedleKeys>
}

impl MemoryStore {

	pub new(space: usize, max_age: Duration) -> MemoryStore {
		MemoryStore {
			total_space: space,
			max_age,

			used_space: 0,
			index: HashMap::new(),
			order: HashMap::new()
		}
	}

	pub fn lookup(&self, keys: NeedleKeys) -> Option<MemoryEntry> {

		let now = SystemTime::now();

		let mut e = match self.index.get_mut(&keys) {
			Some(e) => e,
			None => return None,
		};

		e.last_access = SystemTime::now(),
		e.

	}

	/// Generally to be executed when the 
	pub fn delete(&self, keys: NeedleKeys) {

		

	}


	// Note: Currently deletions are 


}