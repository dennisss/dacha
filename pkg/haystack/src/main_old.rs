#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]
#[macro_use] extern crate rocket;

extern crate dotenv;
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
extern crate mime_sniffer;
extern crate ipnetwork;
extern crate chrono;

extern crate haystack;

use rocket::http::{Status};
use rocket::config::{Config, Environment};
use rocket::fairing::AdHoc;

use haystack::store::routes_helpers::*;
use haystack::directory::Directory;
use haystack::store::machine::StoreMachine;
use haystack::errors::*;

use std::sync::{Arc,Mutex};


// Where to 
const IMAGES_DIR: &str = "./data/picsum";





// We want to know the size of each volume and see whether or not they are diverging from other machines (implying heavy loses or lack of sharding)
// Good to have the total number of images and total size in bytes


// TODO: Ideally also make an error handler for catching and logging (outputting the error as json)


fn main() -> Result<()> {


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

	let mut vol = PhysicalVolume::create(&store, 1)?;

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