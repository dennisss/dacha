

use super::super::common::*;
use super::super::errors::*;
use rocket::http::{Status};
use super::super::store::routes_helpers::*;
use super::super::directory::*;

use std::sync::{Arc,Mutex};

pub type DirectoryHandle<'r> = rocket::State<'r, Arc<Mutex<Directory>>>;
pub type CacheHandle<'r> = rocket::State<'r, Arc<Mutex<Cache>>>;


/// Fetches through the cache an entry
/// NOTE: We can use
#[get("/store/<store_id>/volume/<volume_id>/needle/<key>/<alt_key>?<cookie>")]
fn read_photo(
	dir_handle: DirectoryHandle,
	store_id: MachineId,
	volume_id: VolumeId,
	key: NeedleKey,
	alt_key: NeedleAltKey, 
	cookie: MaybeCookie

) -> Result<HaystackResponse> {

	// Step one is to check in the cache for the pair (inclusive of the )


	// Ideally we'd have the memory store and the cache as two of our 'things'
	// 

	let machine = match dir_handle.lock().unwrap().read_store_machine(store_id)? {
		Some(v) => v,
		None => return Ok(HaystackResponse::Error(Status::NotFound, "No such store machine")),
	};


	/*
	let mut arr: Vec<StoreReadVolumeBody> = vec![];

	for (_, v) in mac.volumes.iter() {
		arr.push(StoreReadVolumeBody::from(v));
	}
	*/

	Ok(HaystackResponse::from(&arr))
}

// TODO: It would be more efficient if we were to provide the list of machines as part of the query as the person requesting this would have to include those anyway
#[post("/volume/<volume_id>/needle/<key>/<alt_key>?<cookie>&<stores>")]
fn upload_photo(
	dir_handle: DirectoryHandle,
	volume_id: VolumeId,
	key: NeedleKey,
	alt_key: NeedleAltKey,
	cookie: MaybeCookie,
	stores: MachineIdList
) {

	// This will be a simulataneous upload and a cache-fill

}
