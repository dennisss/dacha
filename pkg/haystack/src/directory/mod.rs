
pub mod models;
pub mod schema;
mod db;

use super::common::*;
use super::errors::*;
use self::models::*;
use rand;
use rand::prelude::*;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use core::FlipSign;
use self::db::DB;
use std::hash::Hasher;
use std::sync::Arc;


pub struct Directory {

	pub cluster_id: ClusterId,

	pub config: ConfigRef,

	// TODO: Eventually we'd like to make sure that this can become private
	pub db: DB

}


/*
	Directory operations:
	- CreateMachine
	- UpdateMachine
		- Usually a heartbeat to mark the machine as still being alive and recording capacity metrics
		- Also write-enables the machine if it is not already enabled 
	- LockMachine
		- Mark all of a machine's volumes as read-only
		- Take the machine off the list of active volumes
		- Triggered on proper shutdowns and noticing that 

	- DeletePhoto(key, [alt_key])
		-> Drops the 

	- CreatePhoto(key, quantity, sizes)
		-> Returns a cookie, logical_id and a list of stores
		-> Also returns whether or not this change needs to be commited

	- CommitPhoto(key, cookie)
		-> Issue being that if it isn't uplaoded in time, then the old image will be totally dead

*/

impl Directory {

	/// Connects to the backing database and initializes the cluster if needed
	pub fn open(config: Config) -> Result<Directory> {

		let db = DB::connect();

		let cluster_id = match db.get_param(ParamKey::ClusterId as i32)? {
			Some(p) => (&p[..]).read_u64::<LittleEndian>()?,
			None => {

				let id = generate_cluster_id();
				let mut value = vec![];
				value.write_u64::<LittleEndian>(id)?;

				db.create_param(ParamKey::ClusterId as i32, value)?;
				
				id
			}
		};
		

		Ok(Directory {
			db,
			config: Arc::new(config),
			cluster_id
		})		
	}

	pub fn create_logical_volume(&self) -> Result<LogicalVolume> {
		self.db.create_logical_volume(&NewLogicalVolume {
			hash_key: rand::thread_rng().next_u64().flip()
		})
	}


	/// Encapsulates picking/load-balancing the next logical volume for writing to
	/// NOTE: We currently assume that all photos are small enough to fit into a volume such that it will get marked as read-only before any serious overflows start occuring
	/// If there is a failure during uploading, then it should retry with a new volume
	pub fn choose_logical_volume_for_write(&self) -> Result<LogicalVolume> {
		let volumes = self.db.index_logical_volumes()?;

		let avail_vols: Vec<&LogicalVolume> = volumes.iter().filter(|v| {
			v.write_enabled == true
		}).collect();

		if avail_vols.len() == 0 {
			return Err(err_msg("No writeable volumes available"));
		}

		let vol_idx = (rand::thread_rng().next_u32() as usize) % avail_vols.len();
		let vol = avail_vols[vol_idx];

		Ok((*vol).clone())
	}

	/// For a photo, given its volume, picks a cache to use to 
	/// TODO: Eventually this needs to be able to pick between multiples caches per bucket in order to support redundancy in ranges
	pub fn choose_cache(&self, photo: &Photo, vol: &LogicalVolume) -> Result<CacheMachine> {
		if photo.volume_id != vol.id {
			return Err(err_msg("Wrong volume given"))
		}
		
		let mut caches = self.db.index_cache_machines()?.into_iter().filter(|m| {
			m.can_read(&self.config)
		}).collect::<Vec<_>>();

		if caches.len() == 0 {
			return Err(err_msg("Not enough available caches/store"));
		}

		// To pick the cache server, we use a simple Distributed Hash Table approach with a random key per volume
		let mut hasher = siphasher::sip::SipHasher::new_with_keys(vol.hash_key.flip(), 0);
		hasher.write_u64(photo.id.flip());
		let hash = hasher.finish();
		let bucket_size = std::u64::MAX / (caches.len() as u64);
		let mut cache_idx = (hash / bucket_size) as usize; // XXX: Assumes usize is >= u64

		// Handle the case of hitting the max integer value
		if cache_idx >= caches.len() {
			cache_idx = caches.len() - 1;
		}

		// Pick from a sorted list to ensure stability
		caches.sort_by(|a, b| { a.id.cmp(&b.id) });

		let cache = &caches[cache_idx];

		Ok((*cache).clone())
	}

	/// Picks a load balanced store machine from which to read the given photo (to be used only when the cache misses)
	pub fn choose_store(&self, photo: &Photo) -> Result<StoreMachine> {

		let stores = self.db.read_store_machines_for_volume(photo.volume_id.flip())?
		.into_iter().filter(|m| {
			m.can_read(&self.config)
		}).collect::<Vec<_>>();

		if stores.len() == 0 {
			return Err(err_msg("Not enough available caches/store"));
		}

		// Random load balancing
		let mut rng = thread_rng();
		let store = stores.choose(&mut rng).unwrap();

		Ok((*store).clone())
	}

	/// Assign the photo to a new logical volume ideally with a blacklist of machines that we no longer want to use
	/// 
	/// TODO: For efficiency, if uploading successfully reaches some machines, we should prefer to reuse those machines for the next attemp 
	pub fn relocate_photo(blacklist: &Vec<MachineId>) {


		// XXX: For updating an existing image, we do need to first retrieve a volume and cookie assignment and then upload, and then commit it 

		// Uncommited ones are considered abandoned
	}

	// Uploading:
	// - Get the Photo object
	// - Get urls to all stores for the associated logical volume
	// - Perform a POST request to all of them
	// - Retry once on individual failures
	// - On failure, attempt to reassign to a new volume

	// I
	

}

fn generate_cluster_id() -> ClusterId {
	let mut rng = rand::thread_rng();
	rng.next_u64()
}

