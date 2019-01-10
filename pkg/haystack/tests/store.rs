extern crate haystack;

use std::path::Path;
use std::fs;
use haystack::store::volume::*;
use haystack::store::needle::*;
use haystack::store::stream::*;
use haystack::common::*;
use haystack::errors::*;
use haystack::paths::*;


#[test]
fn physical_volume_append() -> Result<()> {

	// TODO: Also clear the index?
	let p = Path::new("out/teststore");
	if p.exists() {
		fs::remove_file(&p);
	}

	// Create new with single needle
	{
		let mut vol = PhysicalVolume::create(&p, 123, 456, 7)?;

		let keys = NeedleKeys { key: 22, alt_key: 3 };

		let r = vol.read_needle(&keys)?;
		assert!(r.is_none());
		assert_eq!(vol.num_needles(), 0);

		let data = vec![1,2,3,4,3,2,1];
		let meta = NeedleMeta {
			flags: 0,
			size: data.len() as NeedleSize
		};
		let cookie = CookieBuf::random();
		let mut strm = SingleStream::from(&data);

		vol.append_needle(keys.clone(), cookie.clone(), meta, &mut strm);


		let r2 = vol.read_needle(&keys)?;
		assert!(r2.is_some());

		let n = r2.unwrap();
		assert_eq!(n.block_offset, 1);
		assert_eq!(n.needle.data(), &data[..]);

		assert_eq!(vol.num_needles(), 1);
	}

	// Reopen
	{
		let mut vol = PhysicalVolume::open(&p)?;
		assert_eq!(vol.superblock.cluster_id, 123);
		assert_eq!(vol.superblock.machine_id, 456);
		assert_eq!(vol.superblock.volume_id, 7);

		assert_eq!(vol.num_needles(), 1);
	}

	Ok(())
}