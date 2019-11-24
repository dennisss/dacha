use super::super::errors::*;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use byteorder::{BigEndian, ReadBytesExt};


const START_OF_IMAGE: &[u8] = &[0xff, 0xd8]; // SOI
const END_OF_IMAGE: u8 = 0xd9; // EOI

const APP0: u8 = 0xe0;

const START_OF_SCAN: u8 = 0xda; // SOS


pub struct JPEG {



}


impl JPEG {

	pub fn open(path: &str) -> Result<JPEG> {
		let mut file = File::open(path)?; 
		
		let mut buf: [u8; 2] = [0; 2];
		file.read_exact(&mut buf)?;
		if buf != START_OF_IMAGE {
			return Err("Invalid start bytes".into());
		}
		assert_eq!(buf, START_OF_IMAGE);

		loop {
			let mut marker = [0u8; 2];
			file.read_exact(&mut marker)?;
			assert_eq!(marker[0], 0xff);
			println!("{:x?}", marker);

			// if 

			if marker[1] == END_OF_IMAGE {
				break;
			}

			let size = file.read_u16::<BigEndian>()?;
			assert!(size > 2); // Must at least contain the size itself.
			file.seek(SeekFrom::Current((size - 2) as i64))?;

			
		}

		Ok(JPEG {})
	}


}