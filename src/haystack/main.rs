#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

#[macro_use] extern crate rocket;
#[macro_use] extern crate rocket_contrib;
#[macro_use] extern crate serde_derive;

extern crate crc32c;
extern crate rand;
extern crate byteorder;
extern crate arrayref;
extern crate hyper;
extern crate futures;
extern crate bytes;
extern crate base64;
extern crate fs2;
extern crate serde;
extern crate serde_json;

mod store;

use std::io;
use std::io::{Write, Cursor};
use std::fs;
use std::fs::{File, read_dir};
use store::common::*;
use store::machine::*;
use store::needle::*;
use store::volume::*;
use rand::prelude::*;
use std::path::Path;
use bytes::Bytes;
use rocket::http::{Status, ContentType};
use rocket::config::{Config, Environment};
use rocket::request::{Request};
use rocket::response::{Response, Responder};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use arrayref::*;

use std::sync::{Arc,Mutex};


// Where to 
const IMAGES_DIR: &str = "./data/picsum";

const VOLUMES_DIR: &str = "./data/hay";

fn generate_id() -> [u8; 16] {
	let mut id = [0u8; 16];
	let mut rng = rand::thread_rng();
	rng.fill_bytes(&mut id);
	id
}


enum HaystackResponse {
	Ok(&'static str),
	Error(Status, &'static str),
	Json(String),
	Needle(HaystackNeedle)
}

impl HaystackResponse {
	fn from<T>(obj: &T) -> HaystackResponse where T: serde::Serialize {
		let body = serde_json::to_string(obj).unwrap();
		HaystackResponse::Json(body)
	}
}

// The quesiton is 

impl<'r> Responder<'r> for HaystackResponse {
    fn respond_to(self, _: &rocket::request::Request) -> rocket::response::Result<'r> {
		match self {
			HaystackResponse::Ok(msg) => {
				Response::build()
					.header(ContentType::Plain)
					.status(Status::Ok)
					.sized_body(Cursor::new(msg.to_owned())).ok()
			},
			HaystackResponse::Error(s, msg) => {
				Response::build()
					.header(ContentType::Plain)
					.sized_body(Cursor::new(msg.to_owned()))
					.status(s).ok()
			},
			HaystackResponse::Json(s) => {
				Response::build()
					.header(ContentType::JSON)
					.sized_body(Cursor::new(s))
					.status(Status::Ok).ok()
			},
			HaystackResponse::Needle(d) => {

				// TODO: Also export the checksum

				let mut cookie = base64::encode_config(&d.header.cookie, base64::URL_SAFE);
				cookie = cookie.trim_end_matches('=').to_string();

				let sum = base64::encode_config(d.crc32c(), base64::URL_SAFE);

				// TODO: Construct an etag based on the rcr, offset and volume_id

				// TODO: Also a header whether the machine is read-only (basically should be managed by the )

				Response::build()
					//.header(ContentType::PNG)
					.header(ContentType::Binary)
					.raw_header("X-Haystack-Cookie", cookie)
					.raw_header("X-Haystack-Hash", String::from("crc32c=") + &sum)
					.sized_body(io::Cursor::new(d.bytes()))
					.ok()
			}
		}
    }
}

struct ContentLength(u64);

impl<'a, 'r> rocket::request::FromRequest<'a, 'r> for ContentLength {
    type Error = &'r str;

    fn from_request(request: &'a Request<'r>) -> rocket::request::Outcome<Self, Self::Error> {
		let s = match request.headers().get_one("Content-Length") {
			Some(s) => s,
			None => {
				return rocket::request::Outcome::Failure((Status::LengthRequired, "Missing Content-Length"))
			}
		};

		let num = match s.parse::<u64>() {
			Ok(n) => n,
			Err(err) => {
				return rocket::request::Outcome::Failure((Status::BadRequest, "Invalid Content-Length"))
			}
		};

		rocket::request::Outcome::Success(ContentLength(num))
	}
}

/// Encapsulates something that may or may not look like a cookie
enum MaybeHaystackCookie {
	Data(Vec<u8>), // Basically should end 
	Invalid
}

impl MaybeHaystackCookie {	
	pub fn from(s: &str) -> MaybeHaystackCookie {
		let r = base64::decode_config(s, base64::URL_SAFE);
		match r {
			Ok(c) => {
				if c.len() != COOKIE_SIZE {
					return MaybeHaystackCookie::Invalid;
				}

				MaybeHaystackCookie::Data(c)
			},
			Err(_) => MaybeHaystackCookie::Invalid
		}
	}

	pub fn data(&self) -> Option<&[u8; COOKIE_SIZE]> {
		match self {
			MaybeHaystackCookie::Data(arr) => Some(array_ref!(arr, 0, COOKIE_SIZE)),
			MaybeHaystackCookie::Invalid => None
		}
	}
}


impl<'r> rocket::request::FromParam<'r> for MaybeHaystackCookie {
	type Error = &'r str;
	fn from_param(param: &'r rocket::http::RawStr) -> Result<Self, Self::Error> {
		Ok(MaybeHaystackCookie::from(param))
	}
}

impl<'v> rocket::request::FromFormValue<'v> for MaybeHaystackCookie {
	type Error = &'v str;
	fn from_form_value(form_value: &'v rocket::http::RawStr) -> Result<Self, Self::Error> {
		Ok(MaybeHaystackCookie::from(form_value))
	}
}






type MachineHandle<'r> = rocket::State<'r, Arc<Mutex<HaystackStoreMachine>>>;

// We want to know the size of each volume and see whether or not they are diverging from other machines (implying heavy loses or lack of sharding)
// Good to have the total number of images and total size in bytes
/*
	Issues:
	- If images are overwritten, how do we gurantee non-stall writes
*/
//#[get("/volumes")]


#[derive(Serialize)]
struct HaystackStoreReadVolumeBody {
	id: u64,
	needles: u64
}

// TODO: 'std::convert::From<&HaystackPhysicalVolume> for'
impl HaystackStoreReadVolumeBody {
	fn from(vol: &HaystackPhysicalVolume) -> HaystackStoreReadVolumeBody {
		HaystackStoreReadVolumeBody {
			id: vol.volume_id,
			needles: vol.len_needles() as u64
		}
	}
}

// Next 

#[get("/volumes")]
fn index_volumes(
	mac_handle: MachineHandle
) -> io::Result<HaystackResponse> {
	let mut mac = mac_handle.lock().unwrap();

	let mut arr: Vec<HaystackStoreReadVolumeBody> = vec![];

	for (id, v) in mac.volumes.iter() {
		arr.push(HaystackStoreReadVolumeBody::from(v));
	}

	Ok(HaystackResponse::from(&arr))
}

#[get("/volume/<volume_id>")]
fn read_volume(
	mac_handle: MachineHandle,
	volume_id: u64
) -> io::Result<HaystackResponse> {
	let mut mac = mac_handle.lock().unwrap();

	match mac.volumes.get(&volume_id) {
		Some(v) =>  Ok(HaystackResponse::from(
			&HaystackStoreReadVolumeBody::from(v)
		)),
		None => Ok(
			HaystackResponse::Error(Status::NotFound, "Volume not found")
		)
	}
}

#[post("/volume/<volume_id>")]
fn create_volume(
	mac_handle: MachineHandle,
	volume_id: u64
) -> io::Result<HaystackResponse> {

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
	volume_id: u64, key: u64, alt_key: u32,
	cookie: Option<MaybeHaystackCookie>

) -> io::Result<HaystackResponse> {

	let mut mac = mac_handle.lock().unwrap();

	let vol = match mac.volumes.get_mut(&volume_id) {
		Some(v) => v,
		None => return Ok(
			HaystackResponse::Error(Status::NotFound, "Volume not found")
		),
	};

	// TODO: I do want to be able to support exporting legit errors
	let r = vol.read_needle(&HaystackNeedleKeys { key, alt_key })?;

	let n = match r {
		Some(n) => n,
		None => {
			return Ok(
				HaystackResponse::Error(Status::NotFound, "Needle not found")
			)
		}
	};

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
	volume_id: u64, key: u64, alt_key: u32,
	data: rocket::Data,
	content_length: ContentLength,
	cookie: MaybeHaystackCookie
) -> io::Result<HaystackResponse> {

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
		&HaystackNeedleKeys { key, alt_key },
		cookie_data,
		&HaystackNeedleMeta { flags: 0, size: content_length.0 },
		&mut strm
	)?;

	Ok(HaystackResponse::Error(Status::Ok, "Needle added!"))
}

#[catch(404)]
fn not_found() -> HaystackResponse {
	HaystackResponse::Error(Status::BadRequest, "Invalid route")
}

// Ideally also make an error handler for catching and logging (outputting the error as json)


fn main() -> io::Result<()> {

	/*
		For eventually creating the cookies
		
		let mut rng = rand::thread_rng();
		let mut cookie = [0u8; COOKIE_SIZE];
		rng.fill_bytes(&mut cookie);
	*/

	let machine = HaystackStoreMachine::load(VOLUMES_DIR)?;
	let mac_handle = Arc::new(Mutex::new(machine));

	let config = Config::build(Environment::Staging)
    .address("127.0.0.1")
    .port(4000)
    .finalize().unwrap();

	rocket::custom(config)
	.mount("/store", routes![
		index_volumes,
		read_volume,
		create_volume,
		read_photo,
		write_photo
	])
	.register(catchers![not_found])
	.manage(mac_handle)
	.launch();

	/*
	let n = match vol.read_needle(&HaystackNeedleKeys { key: 12, alt_key: 0 })? {
		Some(n) => n,
		None => {
			println!("No such needle!");
			return Ok(());
		}
	};

	let mut out = File::create("test.jpg")?;
	out.write_all(n.data())?;
	*/

	/*

	if vol_path.exists() {
		fs::remove_file(vol_path)?;
	}

	let mut vol = HaystackPhysicalVolume::create(&store, 1)?;

	let mut id = 1;
	// Generally we do need to send it back before seeing the checksum, so that may be problematic right
	for entry in read_dir(IMAGES_DIR)? {
		let file = entry?;
		let meta = file.metadata()?;

		println!("{:?}", file.path());

		let key = id; id += 1;
		let alt_key = 0;
		let flags = 0u8;
		let size = meta.len();

		let mut img = File::open(file.path())?;

		vol.append_needle(&HaystackNeedleKeys {
			key, alt_key
		}, &HaystackNeedleMeta {
			size, flags
		}, &mut img)?;


	}

	*/

	println!("Done!");

	Ok(())
}