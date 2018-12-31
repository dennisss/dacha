#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

#[macro_use] extern crate rocket;

extern crate crc32c;
extern crate rand;
extern crate byteorder;
extern crate arrayref;
extern crate hyper;
extern crate futures;
extern crate bytes;
extern crate base64;

mod physical_volume;

use std::io;
use std::io::{Write};
use std::fs;
use std::fs::{File, read_dir};
use physical_volume::*;
use rand::prelude::*;
use std::path::Path;
use bytes::Bytes;
use rocket::http::{Status, ContentType};
use rocket::config::{Config, Environment};
use rocket::request::{Request};
use rocket::response::{Response, Responder};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};



//use hyper::{Body, Request, Response, Server, StatusCode, Chunk};
//use hyper::service::service_fn_ok;
//use hyper::rt::{self, Future};
use std::sync::{Arc,Mutex};


// Where to 
const IMAGES_DIR: &str = "./data/picsum";

const VOLUMES_DIR: &str = "./data/hay";

// Ideally we would have one haystack_index which lists all currently currently active local volumes


fn generate_id() -> [u8; 16] {
	let mut id = [0u8; 16];
	let mut rng = rand::thread_rng();
	rng.fill_bytes(&mut id);
	id
}


enum HaystackResponse {
	Code(Status),
	Data(HaystackNeedle)
}

impl<'r> Responder<'r> for HaystackResponse {
    fn respond_to(self, _: &rocket::request::Request) -> rocket::response::Result<'r> {
		match self {
			HaystackResponse::Code(s) => Response::build().status(s).ok(),
			HaystackResponse::Data(d) => {

				// TODO: Also export the checksum

				let mut cookie = base64::encode_config(&d.header.cookie, base64::URL_SAFE);
				cookie = cookie.trim_end_matches('=').to_string();

				let sum = base64::encode_config(d.stored_crc32c(), base64::URL_SAFE);

				// TODO: Construct an etag based on the rcr, offset and volume_id

				Response::build()
					//.header(ContentType::PNG)
					.raw_header("X-Haystack-Cookie", cookie)
					.raw_header("X-Haystack-Hash", String::from("crc32c=") + &sum)
					.sized_body(io::Cursor::new(d.bytes()))
					.ok()
			}
		}
    }
}

struct ContentLength(u64);
#[derive(Debug)]
enum ContentLengthError {
    Missing,
	Invalid
}

impl<'a, 'r> rocket::request::FromRequest<'a, 'r> for ContentLength {
    type Error = ContentLengthError;

    fn from_request(request: &'a Request<'r>) -> rocket::request::Outcome<Self, Self::Error> {
		let s = match request.headers().get_one("Content-Length") {
			Some(s) => s,
			None => {
				return rocket::request::Outcome::Failure((Status::BadRequest, ContentLengthError::Missing))
			}
		};

		let num = match s.parse::<u64>() {
			Ok(n) => n,
			Err(err) => {
				return rocket::request::Outcome::Failure((Status::BadRequest, ContentLengthError::Invalid))
			}
		};

		rocket::request::Outcome::Success(ContentLength(num))
	}
}

// We want to know the size of each volume and see whether or not they are diverging from other machines (implying heavy loses or lack of sharding)
// Good to have the total number of images and total size in bytes
/*
	Issues:
	- If images are overwritten, how do we gurantee non-stall writes
*/
//#[get("/volumes")]

#[get("/volume/<volume_id>/photo/<key>/<alt_key>?<cookie>")]
fn read_photo(
	vol_handle: rocket::State<Arc<Mutex<HaystackPhysicalVolume>>>,
	volume_id: u64, key: u64, alt_key: u32,
	cookie: Option<String>

) -> io::Result<HaystackResponse> {

	let vol_ref = vol_handle.clone();
	let mut vol = vol_ref.lock().unwrap();

	if volume_id != vol.volume_id {
		return Ok(HaystackResponse::Code(Status::NotFound))
	}

	let r = vol.read_needle(&HaystackNeedleKeys { key, alt_key })?;

	let n = match r {
		Some(n) => n,
		None => {
			return Ok(HaystackResponse::Code(Status::NotFound))
		}
	};

	if let Some(c) = cookie {
		let r = base64::decode_config(&c, base64::URL_SAFE);
		let cookie = match r {
			Ok(c) => c,
			Err(e) => return Ok(HaystackResponse::Code(Status::BadRequest))
		};

		if &cookie != &n.header.cookie {
			return Ok(HaystackResponse::Code(Status::Forbidden))
		}
	}

	Ok(HaystackResponse::Data(n))
}

#[post("/volume/<volume_id>/photo/<key>/<alt_key>", data = "<data>")]
fn write_photo(
	vol_handle: rocket::State<Arc<Mutex<HaystackPhysicalVolume>>>,
	volume_id: u64, key: u64, alt_key: u32,
	data: rocket::Data,
	contentLength: ContentLength
) -> io::Result<HaystackResponse> {

	let vol_ref = vol_handle.clone();
	let mut vol = vol_ref.lock().unwrap();

	if volume_id != vol.volume_id {
		return Ok(HaystackResponse::Code(Status::NotFound))
	}

	let mut strm = data.open();

	vol.append_needle(
		&HaystackNeedleKeys { key, alt_key },
		&HaystackNeedleMeta { flags: 0, size: contentLength.0 },
		&mut strm
	)?;

	Ok(HaystackResponse::Code(Status::Ok))
}


fn main() -> io::Result<()> {

	let store = HaystackClusterConfig {
		cluster_id: generate_id(), 	// TODO: Read from the file if it exists
		volumes_dir: String::from(VOLUMES_DIR)
	};

	let vol_path = Path::new(VOLUMES_DIR).join("haystack_1");

	let vol = if vol_path.exists() {
		HaystackPhysicalVolume::open(vol_path.to_str().unwrap())?
	} else {
		HaystackPhysicalVolume::create(&store, 1)?
	};

	let vol_handle = Arc::new(Mutex::new(vol));


	let config = Config::build(Environment::Staging)
    .address("127.0.0.1")
    .port(4000)
    .finalize().unwrap();

	rocket::custom(config)
	.mount("/store", routes![
		read_photo,
		write_photo
	])
	.manage(vol_handle)
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