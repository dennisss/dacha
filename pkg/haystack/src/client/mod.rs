

/*
	Mainly implements uploading and fetching o urls from the directory library

*/

use super::errors::*;
use super::common::*;
use super::directory::*;
use super::paths::*;
use super::store::route_write::StoreWriteBatchResponse;
use bitwise::Word;
use bytes::Bytes;
use futures::future;
use futures::prelude::*;
use futures::stream;
use futures::future::*;
use std::sync::Arc;
use std::io::Cursor;
use futures::{Async, Poll};
use futures::prelude::*;
use futures::prelude::await;
use futures::Stream;
use byteorder::{LittleEndian, ReadBytesExt};

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

		let cookie = CookieBuf::from(&p.cookie[..]);

		let needles = chunks.iter().map(|c| {
			NeedleChunk {
				path: NeedleChunkPath {
					volume_id: p.volume_id.to_unsigned(),
					key: p.id.to_unsigned(),
					alt_key: c.alt_key,
					cookie: cookie.clone()
				},
				data: c.data.clone()
			}
		}).collect::<Vec<_>>();

		let num = needles.len();

		let arr = machines.into_iter().map(move |m| {
			let needles = (&needles[..]).to_vec();
			let m = Arc::new(m);

			//Client::upload_needle_sequential(&m, needles)
			Client::upload_needle_batch(&m, &needles)
			.and_then(move |n| {
				if num != n {
					return err("Not all chunks uploaded".into());
				}

				ok(())
			})

		}).collect::<Vec<_>>();

		Box::new(join_all(arr).map(move |_| {
			p2.id.to_unsigned()
		}))
	}

	/// Uploads many chunks using traditional sequential requests (flushed after every single request)
	/// TODO: Currently this will never respond with a partial count
	fn upload_needle_sequential(mac: &models::StoreMachine, chunks: Vec<NeedleChunk>)
		-> impl Future<Item=usize, Error=Error> {

		let client = hyper::Client::new();

		let addr = mac.addr();
		let mac_id = mac.id as MachineId;

		// Better tofold and then combine
		stream::iter_ok(chunks).fold(0, move |num, c| {
			let url = format!(
				"http://{}{}",
				addr,
				StorePath::Needle {
					volume_id: c.path.volume_id,
					key: c.path.key,
					alt_key: c.path.alt_key,
					cookie: c.path.cookie
				}.to_string()
			);

			let req = hyper::Request::builder()
				.uri(&url)
				.method("POST")
				.header("Host", Host::Store(mac_id).to_string())
				.body(hyper::Body::from(c.data.clone()))
				.unwrap();

			// Make request, change error type to out error type
			client.request(req)
			.map_err(|e| Error::from(e))
			.and_then(move |resp| {
				if !resp.status().is_success() {
					// TODO: Also log out the actual body message?
					return err(format!("Received status {:?} while uploading", resp.status()).into());
				}

				ok(num + 1)
			})
		})
		.and_then(|num| ok(num))
	}

	/// Uploads some number of chunks to a single machine/volume and returns how many of the chunks succeeded in being flushed to the volume
	fn upload_needle_batch(mac: &models::StoreMachine, chunks: &[NeedleChunk])
		-> impl Future<Item=usize, Error=Error> {
		
		let mut body_chunks = vec![];

		for c in chunks {
			let mut header = vec![];
			c.write_header(&mut Cursor::new(&mut header)).expect("Failure making chunk header");

			body_chunks.push(hyper::Chunk::from(Bytes::from(header)));
			body_chunks.push(hyper::Chunk::from(c.data.clone()));
		}

		let s = stream::iter_ok::<_, std::io::Error>(body_chunks);

		let url = format!(
			"http://{}{}",
			mac.addr(),
			StorePath::Index.to_string()
		);

		let client = hyper::Client::new();
		let req = hyper::Request::builder()
			.uri(url)
			.method("PATCH")
			.header("Host", Host::Store(mac.id as MachineId).to_string())
			.body(hyper::Body::wrap_stream(s))
			.unwrap();
	
		client.request(req)
		.map_err(|e| e.into())
		.and_then(|resp| {
			if !resp.status().is_success() {
				return err(format!("Request failed with code: {}", resp.status()).into());
			}

			ok(resp)
		})
		.and_then(|resp| {
			resp.into_body()
			.map_err(|e| e.into())
			.fold(Vec::new(), |mut buf, c| -> FutureResult<Vec<_>, Error> {
				buf.extend_from_slice(&c);
				ok(buf)
			})
			.and_then(|buf| {
				
				let res = match serde_json::from_slice::<StoreWriteBatchResponse>(&buf) {
					Ok(v) => v,
					Err(_) => return err("Invalid json response received".into())
				};

				if let Some(e) = res.error {
					eprintln!("Upload error: {:?}", e);
				}

				let num = res.num_written;

				ok(num as usize)
			})
		})
	}

	pub fn get_photo_cache_url() {
		// This is where the distributed hashtable stuff will come into actual
	}

	pub fn get_photo_store_url() {

	}

}

