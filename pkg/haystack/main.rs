#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

#[macro_use] extern crate rocket;
#[macro_use] extern crate serde_derive;

#[macro_use] extern crate diesel;

extern crate dotenv;
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

use diesel::prelude::*;
use diesel::pg::PgConnection;
use dotenv::dotenv;
use std::env;


mod store;
mod directory;

use std::io;
use std::fs;
use store::machine::*;
use rand::prelude::*;
use rocket::http::{Status};
use rocket::config::{Config, Environment};
use store::routes_helpers::*;

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






// We want to know the size of each volume and see whether or not they are diverging from other machines (implying heavy loses or lack of sharding)
// Good to have the total number of images and total size in bytes
/*
	Issues:
	- If images are overwritten, how do we gurantee non-stall writes
*/
//#[get("/volumes")]

#[catch(404)]
fn not_found() -> HaystackResponse {
	HaystackResponse::Error(Status::BadRequest, "Invalid route")
}

/*
	Realistically no reason the directory could not just be in Go
	- Simpler to manage that way probably

*/

// TODO: Ideally also make an error handler for catching and logging (outputting the error as json)

pub fn establish_connection() -> PgConnection {
	dotenv().ok();

	let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
	PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}


fn main() -> io::Result<()> {

	let conn = establish_connection();

	/*

	/pkg/haystack/[directory]

	// Will likely end up as datalayer/pkg/haystack/

	// /projects/haystack

	// XXX: 

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
	.mount("/store", store::routes::get())
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