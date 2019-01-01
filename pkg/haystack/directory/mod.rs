
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
use self::db::*;

pub struct Directory {

	pub cluster_id: ClusterId,

	db: DB

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

	/// Creates a new machine for 
	/// 
	/// NOTE: We assume that this is being called on the 
	pub fn create_store_machine(&self) -> Result<StoreMachine> {
		self.db.create_store_machine("127.0.0.1", 4000)
	}

	/// For a photo, this will retrieve a url where it can be read from 
	/// For now we will directly hit the cache for all operations
	pub fn read_photo() {

	}

	/// Creates a new photo with an initial volume assignment but not uploaded yet
	pub fn create_photo() {

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


