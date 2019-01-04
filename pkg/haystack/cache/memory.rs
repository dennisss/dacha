use super::super::common::*;
use super::super::store::needle::*;
use std::collections::HashMap;
use std::collections::BTreeMap;
use std::time::*;
use hyper::http::HeaderMap;

pub struct MemoryEntry {

	pub cookie: Cookie,

	/// The logical volume from which we got this entry from originally
	/// Because photos can change logical volumes upon overflow of their old ones, this may change over time
	/// TODO: Because we don't include this in the cache key, sequential attempts to lookup different volumes will cache miss a lot, so ideally we hope that all browsers quickly pick up the new volume id
	/// Additionally because the old volume will typically reside of a read-only machine, we will typically not end up recaching old versions
	pub logical_id: VolumeId,

	/// We will opaquely cache most custom and cache-related headers that we get back from the store (mainly to replicate the original response we got from the store)
	pub headers: HeaderMap,

	/// THis will be the raw data of the needle file it is associated with 
	pub data: Vec<u8>,
}


// TODO: Should we retain a cache of deleted entries (just the flags)

struct MemoryEntryInternal {

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
	pub total_space: usize,

	/// Size of the largest entry that we will bother trying to cache
	max_entry_size: usize,

	max_age: Duration,


	/// Amount of memory in bytes used up by all cache entries (excluding the metadata needed to store them)
	pub used_space: usize,

	/// TODO: The real question will be how to deal with 
	index: HashMap<NeedleKeys, MemoryEntryInternal>,

	// TODO: In case it switches volumes, 
	order: BTreeMap<SystemTime, NeedleKeys>
}

impl MemoryStore {

	pub fn new(total_space: usize, max_entry_size: usize, max_age: Duration) -> MemoryStore {
		MemoryStore {
			total_space,
			max_entry_size,
			max_age,

			used_space: 0,
			index: HashMap::new(),
			order: BTreeMap::new()
		}
	}

	pub fn lookup(&self, keys: NeedleKeys) -> Option<MemoryEntry> {

		let now = SystemTime::now();

		let mut e = match self.index.get_mut(&keys) {
			Some(e) => e,
			None => return None,
		};

		// Check not stale
		if self.is_stale(e, &now) {
			self.delete(&keys, &e);
			return None;
		}

		e.last_access = now;

		Some(e.data)
	}

	pub fn insert(&self, keys: NeedleKeys, entry: MemoryEntry) {

		// Delete any old one
		self.remove(&keys);
		
		// Don't try inserting entries that are too large
		if entry.data.len() > self.max_entry_size {
			return;
		}

		// Allocate space for it
		self.used_space += entry.data.len();

		// Make sure we have enough space for it
		self.collect();

		let now = SystemTime::now();
		
		self.index.insert(keys, MemoryEntryInternal {
			data: entry,
			inserted_at: now,
			last_access: now,
			last_order: now
		});

		self.order.insert(now, keys);
	}

	/// Explicit removal of an entry (usually if we the cache is the one that performed the deletion)
	pub fn remove(&self, keys: &NeedleKeys) {
		if let Some(e) = self.index.get(keys) {
			self.delete(&keys, e);
		}
	}

	pub fn len(&self) -> usize {
		self.index.len()
	}

	fn collect(&self) {
		
		let now = SystemTime::now();

		let mut try = true;
		let mut nremoved = 0;

		while try {
			try = false;

			for (time, keys) in self.order.iter_mut() {
				let mut e = self.index.get_mut(keys).unwrap();

				// Was accessed since we last indexed it
				if e.last_access != *time {
					*time = e.last_access;
					e.last_order = e.last_access;
					try = true; // < I don't know which position rust will go to after we move it, so to be safe, we will retry from the beginning of the loop (especially because we do want to re-run it on ourselves)
					break;
				}

				if self.is_stale(e, &now) || self.need_space() {
					self.delete(&keys, &e);
					nremoved += 1;
				}
				else {
					break;
				}	
			}
		}

		// TODO: Validate that we definately have enough space now

		println!("Removed {} cache keys", nremoved);
	}

	fn is_stale(&self, e: &MemoryEntryInternal, now: &SystemTime) -> bool {
		now.duration_since(e.inserted_at).unwrap_or(Duration::from_millis(0)).ge(&self.max_age)
	}

	fn need_space(&self) -> bool {
		self.used_space > self.total_space
	}

	fn delete(&self, keys: &NeedleKeys, entry: &MemoryEntryInternal) {
		self.index.remove(keys);
		self.order.remove(&entry.last_order);
		self.used_space = self.used_space - entry.data.data.len();
	}



}