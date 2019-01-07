
pub mod models;
pub mod schema;
mod db;

use super::common::*;
use super::errors::*;
use self::models::*;
use rand;
use rand::prelude::*;
use std::mem::size_of;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use bitwise::Word;
use self::db::DB;

pub struct Directory {

	pub cluster_id: ClusterId,

	// TODO: Eventually we'd like to make sure that this can become private
	pub db: DB

}

impl Directory {

	/// Connects to the backing database and initializes the cluster if needed
	pub fn open() -> Result<Directory> {

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
			cluster_id
		})		
	}

	pub fn read_store_machine(&self, id: MachineId) -> Result<Option<StoreMachine>> {
		self.db.read_store_machine(id)
	}

	pub fn create_logical_volume(&self) -> Result<LogicalVolume> {
		self.db.create_logical_volume(&NewLogicalVolume {
			hash_key: rand::thread_rng().next_u64().to_signed()
		})
	}

	/// 

	/// For a photo, this will retrieve a url where it can be read from 
	/// For now we will directly hit the cache for all operations
	//pub fn read_photo() -> Result<Photo> {
	//	
	//}

	// NOTE: We currently do not support any ability to 

	/// Creates a new photo with an initial volume assignment but not uploaded yet
	/// NOTE: We currently assume all of the photos are small enough to fit into a volume
	/// If there is a failure during uploading, then it should retry with a new volume
	pub fn create_photo(&self) -> Result<Photo> {

		let volumes = self.db.index_logical_volumes()?;

		let avail_vols: Vec<&LogicalVolume> = volumes.iter().filter(|v| {
			v.write_enabled == true
		}).collect();

		if avail_vols.len() == 0 {
			return Err("No writeable volumes available".into());
		}

		let vol_idx = (rand::thread_rng().next_u32() as usize) % avail_vols.len();
		let vol = avail_vols[vol_idx];

		let p = self.db.create_photo(&NewPhoto {
			volume_id: vol.id,
			cookie: &generate_cookie()
		})?;

		Ok(p)
	}

	/// Assign the photo to a new logical volume ideally with a blacklist of machines that we no longer want to use
	/// 
	/// TODO: For efficiency, if uploading successfully reaches some machines, we should prefer to reuse those machines for the next attemp 
	pub fn relocate_photo(blacklist: &Vec<MachineId>) {

	}

	/*
		Creating a volume
		- We will insert the volume into the database (as not write-enabled)
		- We will then ping store servers and create the volume
		- We will then insert the machines as volume assignments
		- Finally we will mark the volume as insertable
	*/


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

fn generate_cookie() -> Cookie {
	let mut id = [0u8; size_of::<Cookie>()];
	let mut rng = rand::thread_rng();
	rng.fill_bytes(&mut id);
	id
}


