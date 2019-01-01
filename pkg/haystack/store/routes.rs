use super::super::common::*;
use super::super::errors::*;
use super::machine::*;
use super::volume::*;
use super::needle::*;
use rocket::http::{Status};
use super::routes_helpers::*;

use std::sync::{Arc,Mutex};



pub type MachineHandle<'r> = rocket::State<'r, Arc<Mutex<StoreMachine>>>;


#[derive(Serialize)]
struct StoreReadVolumeBody {
	id: VolumeId,
	needles: usize
}

// TODO: 'std::convert::From<&PhysicalVolume> for'
impl StoreReadVolumeBody {
	fn from(vol: &PhysicalVolume) -> StoreReadVolumeBody {
		StoreReadVolumeBody {
			id: vol.volume_id,
			needles: vol.len_needles()
		}
	}
}

#[get("/volumes")]
fn index_volumes(
	mac_handle: MachineHandle
) -> Result<HaystackResponse> {
	let mac = mac_handle.lock().unwrap();

	let mut arr: Vec<StoreReadVolumeBody> = vec![];

	for (_, v) in mac.volumes.iter() {
		arr.push(StoreReadVolumeBody::from(v));
	}

	Ok(HaystackResponse::from(&arr))
}

#[get("/volume/<volume_id>")]
fn read_volume(
	mac_handle: MachineHandle,
	volume_id: VolumeId
) -> Result<HaystackResponse> {
	let mac = mac_handle.lock().unwrap();

	match mac.volumes.get(&volume_id) {
		Some(v) =>  Ok(HaystackResponse::from(
			&StoreReadVolumeBody::from(v)
		)),
		None => Ok(
			HaystackResponse::Error(Status::NotFound, "Volume not found")
		)
	}
}

#[post("/volume/<volume_id>")]
fn create_volume(
	mac_handle: MachineHandle,
	volume_id: VolumeId
) -> Result<HaystackResponse> {

	let mut mac = mac_handle.lock().unwrap();

	if mac.volumes.contains_key(&volume_id) {
		return Ok(HaystackResponse::Ok("Volume already exists"));
	}

	mac.create_volume(volume_id)?;

	Ok(HaystackResponse::Error(Status::Created, "Volume created!"))
}


#[get("/volume/<volume_id>/needle/<key>/<alt_key>?<cookie>")]
fn read_photo(
	mac_handle: MachineHandle,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey,
	cookie: Option<MaybeCookie>

) -> Result<HaystackResponse> {

	let mut mac = mac_handle.lock().unwrap();

	let vol = match mac.volumes.get_mut(&volume_id) {
		Some(v) => v,
		None => return Ok(
			HaystackResponse::Error(Status::NotFound, "Volume not found")
		),
	};

	// TODO: I do want to be able to support exporting legit errors
	let r = vol.read_needle(&NeedleKeys { key, alt_key })?;

	let n = match r {
		Some(n) => n,
		None => {
			return Ok(
				HaystackResponse::Error(Status::NotFound, "Needle not found")
			)
		}
	};

	// Integrity check
	n.check()?;

	if let Some(c) = cookie {
		let arr = match c.data() {
			Some(arr) => arr,
			None => return Ok(HaystackResponse::Error(Status::BadRequest, "Malformed cookie")),
		};

		if arr != &n.header.cookie {
			return Ok(HaystackResponse::Error(Status::Forbidden, "Incorrect cookie"));
		}
	}
	else {
		// Cookie was not given
		// If we are configured to require cookies, then this would be a mistake
	}

	Ok(HaystackResponse::Needle(n))
}

#[post("/volume/<volume_id>/needle/<key>/<alt_key>?<cookie>", data = "<data>")] // TODO: ?<cookie>
fn write_photo(
	mac_handle: MachineHandle,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey,
	data: rocket::Data,
	content_length: ContentLength,
	cookie: MaybeCookie
) -> Result<HaystackResponse> {

	let mut mac = mac_handle.lock().unwrap();

	let vol = match mac.volumes.get_mut(&volume_id) {
		Some(v) => v,
		None => return Ok(HaystackResponse::Error(Status::NotFound, "Volume not found")),
	};

	let cookie_data = match cookie.data() {
		Some(arr) => arr,
		None => return Ok(HaystackResponse::Error(Status::BadRequest, "Malformed cookie")),
	};

	let mut strm = data.open();

	// TODO: If a needle already exists with the exact same key and cookie, then we can ignore it

	vol.append_needle(
		&NeedleKeys { key, alt_key },
		cookie_data,
		&NeedleMeta { flags: 0, size: content_length.0 },
		&mut strm
	)?;

	Ok(HaystackResponse::Error(Status::Ok, "Needle added!"))
}

pub fn get() -> Vec<rocket::Route> {
	routes![
		index_volumes,
		read_volume,
		create_volume,
		read_photo,
		write_photo
	]
}

