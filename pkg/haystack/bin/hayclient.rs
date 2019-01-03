/*
	A simple cli client for performing CRUD operations against Haystack servers

	Usage:
	- Uploading
		hayclient upload [alt_key] [filename]

	- Reading back
		hayclient cache-url [key] [alt_key]
		hayclient store-url [key] [alt_key]
*/

extern crate haystack;
use haystack::errors::*;
use haystack::common::*;
use haystack::client::*;
use std::env;
use std::io::{Read};

use std::fs::File;

fn main() -> Result<()> {
	println!("client");
	let args: Vec<String> = env::args().collect();

	if &args[1] == "upload" {

		println!("Starting upload");

		let alt_key = args[2].parse::<NeedleAltKey>().unwrap();
		let filename = &args[3];

		let mut f = File::open(filename)?;
		let mut data = vec![];
		f.read_to_end(&mut data)?;

		let chunks = vec![
			PhotoChunk {
				alt_key,
				data
			}
		];

		let c = haystack::client::Client::create()?;

		let pid = c.upload_photo(chunks)?;

		println!("Uploaded with photo id: {}", pid);
	}

	Ok(())
}