

/*
	Mainly implements uploading and fetching o urls from the directory library

*/

use super::errors::*;
use super::common::*;
use super::directory::*;
use super::paths::*;
use bitwise::Word;
use base64;
use reqwest;
use bytes::Bytes;

pub struct Client {
	dir: Directory 
}

pub struct PhotoChunk {
	pub alt_key: NeedleAltKey,
	pub data: Bytes
}

impl Client {

	pub fn create() -> Result<Client> {
		Ok(Client {
			dir: Directory::open()?
		})
	}

	/// Creates a new photo containing all of the given chunks
	/// TODO: On writeability errors, relocate the photo to a new volume that doesn't have the given machines
	pub fn upload_photo(&self, chunks: Vec<PhotoChunk>) -> Result<NeedleKey> {
		assert!(chunks.len() > 0);

		let p = self.dir.create_photo()?;

		let machines = self.dir.db.read_store_machines_for_volume(p.volume_id.to_unsigned())?;
		if machines.len() == 0 {
			return Err("Missing any machines to upload to".into())
		}

		// NOTE: I do need to know 

		for m in machines.iter() {
			if !m.can_write() {
				return Err("Some machines are not writeable".into())
			}
		}

		// TODO: Will eventually need to make these all parallel task with retrying once and a bail-out on all failures
		for m in machines.iter() {

			let client = reqwest::Client::new();	

			for c in chunks.iter() {
				let url = format!(
					"http://{}{}",
					m.addr(),
					StorePath::Needle {
						volume_id: p.volume_id.to_unsigned(),
						key: p.id.to_unsigned(),
						alt_key: c.alt_key,
						cookie: CookieBuf::from(&p.cookie)
					}.to_string()
					
				);

				// TODO: This will usually be an expensive clone and not good for us
				let resp = client
					.post(&url)
					.header("Host", Host::Store(m.id as MachineId).to_string())
					.body(reqwest::Body::new(std::io::Cursor::new(c.data.clone()))).send()?;
				if !resp.status().is_success() {
					// TODO: Also log out the actual body message?
					return Err(format!("Received status {:?} while uploading", resp.status()).into());
				}
			}

		}

		Ok(p.id.to_unsigned())
	}

	pub fn get_photo_cache_url() {
		// This is where the distributed hashtable stuff will come into actual
	}

	pub fn get_photo_store_url() {

	}

}

