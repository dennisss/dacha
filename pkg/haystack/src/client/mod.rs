

/*
	Mainly implements uploading and fetching o urls from the directory library

*/

use super::errors::*;
use super::common::*;
use super::directory::*;
use super::paths::*;
use bitwise::Word;
use base64;
use bytes::Bytes;
use futures::future;
use futures::prelude::*;
use futures::stream;
use futures::future::*;
use std::sync::Arc;
use std::cell::Cell;

pub struct Client {
	dir: Directory 
}

#[derive(Clone)]
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
	pub fn upload_photo(&self, chunks: Vec<PhotoChunk>) -> Box<Future<Item=NeedleKey, Error=Error> + Send> {
		assert!(chunks.len() > 0);

		let p = match self.dir.create_photo() { Ok(v) => Arc::new(v), Err(e) => return Box::new(err(e)) };
		let p2 = p.clone();

		let machines = match self.dir.db.read_store_machines_for_volume(p.volume_id.to_unsigned()) {
			Ok(v) => v,
			Err(e) => return Box::new(err(e))
		};

		if machines.len() == 0 {
			return Box::new(err("Missing any machines to upload to".into()))
		}

		for m in machines.iter() {
			if !m.can_write() {
				return Box::new(err("Some machines are not writeable".into()))
			}
		}

		// TODO: Will eventually need to make these all parallel task with retrying once and a bail-out on all failures

		let arr = machines.into_iter().map(enclose!((p) move |m| {
			let chunks = (&chunks[..]).to_vec();
			let m = Arc::new(m);

			let client = hyper::Client::new();		

			stream::iter_ok(chunks).for_each(enclose!((p, m) move |c| {
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

				let req = hyper::Request::builder()
					.uri(&url)
					.method("POST")
					.header("Host", Host::Store(m.id as MachineId).to_string())
					.body(hyper::Body::from(c.data.clone()))
					.unwrap();

				// Make request, change error type to out error type
				client.request(req)
				.map_err(|e| e.into())
				.and_then(|resp| {
					if !resp.status().is_success() {
						// TODO: Also log out the actual body message?
						return err(format!("Received status {:?} while uploading", resp.status()).into());
					}

					ok(())
				})
			}))

		})).collect::<Vec<_>>();

		Box::new(join_all(arr).map(move |_| {
			p2.id.to_unsigned()
		}))
	}

	pub fn get_photo_cache_url() {
		// This is where the distributed hashtable stuff will come into actual
	}

	pub fn get_photo_store_url() {

	}

}

