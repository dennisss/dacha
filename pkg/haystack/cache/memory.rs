use super::super::common::*;
use super::super::paths::CookieBuf;
use std::collections::HashMap;
use std::collections::BTreeMap;
use std::time::*;
use hyper::http::HeaderMap;
use bytes::Bytes;
use std::sync::Arc;


/// NOTE: Everything in this entry is essentially immutable
pub struct MemoryEntry {

	/// When this entry was inserted
	pub inserted_at: SystemTime,

	pub cookie: CookieBuf,

	/// The logical volume from which we got this entry from originally
	/// Because photos can change logical volumes upon overflow of their old ones, this may change over time
	/// TODO: Because we don't include this in the cache key, sequential attempts to lookup different volumes will cache miss a lot, so ideally we hope that all browsers quickly pick up the new volume id
	/// Additionally because the old volume will typically reside of a read-only machine, we will typically not end up recaching old versions
	pub logical_id: VolumeId,

	/// We will opaquely cache most custom and cache-related headers that we get back from the store (mainly to replicate the original response we got from the store)
	pub headers: HeaderMap,

	/// THis will be the raw data of the needle file it is associated with 
	pub data: Bytes,
}

pub enum Cached {
	Valid(Arc<MemoryEntry>),
	Stale(Arc<MemoryEntry>),
	None
}

// TODO: Should we retain a cache of deleted entries (just the flags)

#[derive(Clone)]
struct MemoryEntryInternal {

	// XXX: Hello world, should this be separately referenced
	pub value: Arc<MemoryEntry>,

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

	// Issue being that i can't mutate these nicely
	index: HashMap<NeedleKeys, MemoryEntryInternal>,

	// TODO: In case it switches volumes, 
	order: BTreeMap<SystemTime, NeedleKeys>
}


/*
	What to use as the ETag
	- Combine the length, crc32c
*/

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

	pub fn lookup(&mut self, keys: &NeedleKeys) -> Cached {

		let now = SystemTime::now();

		let mut e = match self.index.get(keys) {
			Some(e) => e.clone(),
			None => return Cached::None,
		};

		// If stale, then we should delete it from the table
		if self.is_stale(&e, &now) {
			// In this case we may return a cache result get for the hell of it

			self.delete(keys, &e);

			Cached::Stale(e.value)
		}
		else {
			// Grab a reference to the data to return
			let out = e.value.clone();

			// Update last accessed time and put it back
			e.last_access = now;
			self.index.insert(keys.clone(), e);

			Cached::Valid(out)
		}
	}

	pub fn insert(&mut self, keys: NeedleKeys, entry: Arc<MemoryEntry>) {

		// Delete any old one
		self.remove(&keys);
		
		// Don't try inserting entries that are too large
		if entry.data.len() > self.max_entry_size {
			println!("Not caching entry: too large ({} > {})", entry.data.len(), self.max_entry_size);
			return;
		}

		// Allocate space for it
		self.used_space += entry.data.len();

		// Make sure we have enough space for it
		self.collect();
		
		let now = entry.inserted_at.clone();

		self.index.insert(keys.clone(), MemoryEntryInternal {
			value: entry,
			last_access: now,
			last_order: now
		});

		self.order.insert(now, keys);
	}

	/// Explicit removal of an entry (usually if we the cache is the one that performed the deletion)
	pub fn remove(&mut self, keys: &NeedleKeys) {
		if let Some(e) = self.index.get(keys).cloned() {
			self.delete(keys, &e);
		}
	}

	pub fn len(&self) -> usize {
		self.index.len()
	}

	fn collect(&mut self) {
		
		let mut nremoved = 0;

		loop {
			// Get the first item with lowest time
			let (time, keys) = match self.order.iter().next() {
				Some((t, k)) => (t.clone(), k.clone()),
				None => break
			};

			// Look up the corresponding entry
			let mut e = self.index.get(&keys).unwrap().clone();

			// If we accessed it since the last time it was ordered, we will re-order it
			if e.last_access != time {
				self.order.remove(&time);
				self.order.insert(e.last_access, keys.clone());

				e.last_order = e.last_access;
				self.index.insert(keys, e);
			}
			// Perform garbage collection if needed
			// NOTE: We will keep stale entries under the assumption that are likely immutable and that on the next wrap of it, it will become 
			else if /* self.is_stale(&e, &now) || */ self.need_space() {
				self.delete(&keys, &e);
				nremoved += 1;
			}
			else {
				break;
			}
		}

		// TODO: Validate that we definately have enough space now

		println!("Removed {} cache keys", nremoved);
	}

	fn is_stale(&self, e: &MemoryEntryInternal, now: &SystemTime) -> bool {
		now.duration_since(e.value.inserted_at).unwrap_or(Duration::from_millis(0)).ge(&self.max_age)
	}

	fn need_space(&self) -> bool {
		self.used_space > self.total_space
	}

	// Simple answer is to just clone it as that is reasonable cheap
	fn delete(&mut self, keys: &NeedleKeys, entry: &MemoryEntryInternal) {
		self.index.remove(keys);
		self.order.remove(&entry.last_order);
		self.used_space = self.used_space - entry.value.data.len();
	}



}