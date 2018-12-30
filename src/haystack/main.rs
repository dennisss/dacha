
extern crate crc32c;
extern crate rand;
extern crate byteorder;
extern crate arrayref;

mod physical_volume;

use std::io;
use std::io::{Write};
use std::fs;
use std::fs::{File, read_dir};
use physical_volume::*;
use rand::prelude::*;
use std::path::Path;


// Where to 
const IMAGES_DIR: &str = "./data/picsum";

const VOLUMES_DIR: &str = "./data/hay";

// Ideally we would have one haystack_index which lists all currently currently active local volumes

/*
	
	Haystack Store
		- 100's of physical volumes of size 100GB on one machine
		- Many physical volumes across machines grouped as logical volumes
		- Each physical volume in a logical volume is effectively a replica

		- Logical volumes stored in:
			- ‘/hay/haystack_<logical volume id>'

		- For each volume, we should have an in-memory mapping of:
			- (key, alt_key) -> Needle(flags, size, offset)
			- ^ this mapping is built during startup by scanning the file

	Store Operations
		Photo Read
			- args: logical volume id, key, alternate key, and cookie

		Photo Write
			- Synchronous and append only
			- overwriting is an append of a new version taking the highest offset of a given key/alt_key pair to be the newest

		Photo Delete
			- Mark the flag and in-memory mapping with the change
			- TODO: Potential issues here with keeping everything as append-only

	Directory
		- logical to physical mapping
		- application metadata
		- info on which physical volume is photo is stored in 
		- Serves users in creating urls that point to the cache
			- of the form:
				http://⟨CDN⟩/⟨Cache⟩/⟨Machine id⟩/⟨Logical volume, Photo⟩
		- Pretty much implemented on top of some replicated database
	
	Cache
		- 

*/

fn generate_id() -> [u8; 16] {
	let mut id = [0u8; 16];
	let mut rng = rand::thread_rng();
	rng.fill_bytes(&mut id);
	id
}


fn main() -> io::Result<()> {

	// Make a rust http server around it now

	let store = HaystackClusterConfig {
		cluster_id: generate_id(),
		volumes_dir: String::from(VOLUMES_DIR)
	};

	let vol_path = Path::new(VOLUMES_DIR).join("haystack_1");

	
	let mut vol = HaystackPhysicalVolume::open(vol_path.to_str().unwrap())?;
	
	let n = match vol.read_needle(&HaystackNeedleKeys { key: 12, alt_key: 0 })? {
		Some(n) => n,
		None => {
			println!("No such needle!");
			return Ok(());
		}
	};

	let mut out = File::create("test.jpg")?;
	out.write_all(n.data())?;

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