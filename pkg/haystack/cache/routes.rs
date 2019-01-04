

use super::super::common::*;
use super::super::http::*;
use super::super::errors::*;
use super::machine::*;
use super::memory::*;
use futures::prelude::*;
use super::super::paths::*;
use hyper::{Body, Response, Method, StatusCode};
use hyper::http::request::Parts;
use hyper::body::Payload;
use std::sync::{Arc,Mutex};
use futures::prelude::*;
use futures::prelude::await;


type MachineHandle = Arc<Mutex<CacheMachine>>;


#[async]
pub fn handle_request(
	parts: Parts, body: Body, mac_handle: MachineHandle
) -> Result<Response<Body>> {

	let segs = match split_path_segments(&parts.uri.path()) {
		Some(v) => v,
		None => return Ok(bad_request())
	};

	// We should not be getting any query parameters
	if parts.uri.query() != None {
		return Ok(bad_request());
	}

	let params = match CachePath::from(&segs) {
		Ok(v) => v,
		Err(s) => return Ok(text_response(StatusCode::BAD_REQUEST, s))
	};

	match params {

		CachePath::Index => Ok(bad_request()),

		CachePath::Proxy { machine_ids, store } => {
			await!(handle_proxy_request(parts, body, mac_handle, machine_ids, store))
		},


		_ => Ok(bad_request())
	}

}

#[derive(Serialize)]
pub struct CacheIndexResponse {
	pub used_space: usize,
	pub total_space: usize,
	pub num_entries: usize
}

// TODO: This should probably not be exposeable to random external clients
fn index_cache(mac_handle: MachineHandle) -> Result<Response<Body>> {
	let mac = mac_handle.lock().unwrap();

	// TODO: Would also key good to hashed key-range of this cache
	Ok(json_response(StatusCode::OK, &CacheIndexResponse {
		used_space: mac.memory.used_space,
		total_space: mac.memory.total_space,
		num_entries: mac.memory.len()
	}))
}

// 
// Whether or not it is from the 

/*
	Things to know about each request
	- Whether or not it came from the CDN
	- Whether or not it is internal
*/

use rand::thread_rng;
use rand::seq::SliceRandom;
use reqwest::header::HeaderMap;
use futures::future;

/// To mitigate backend DOS, this will limit the number of machines that can be specified as backends when making a request to the cache (does not apply in the unspecified mode)
const MAX_MACHINE_LIST_SIZE: usize = 6;

#[async]
fn handle_proxy_request(
	parts: Parts, body: Body, mac_handle: MachineHandle, machine_ids: MachineIds, store: StorePath
) -> Result<Response<Body>> {

	// Step one is to check in the cache for the pair (inclusive of the )
	// Check if an If-None-Match is given, etc.

	// Will get the list of store machine addresses that for for this
	let get_backend_addrs = move |mac: &CacheMachine, volume_id: VolumeId| -> Result<Vec<String>> {
		// TODO: Limit the maximum number of 

		let macs = match machine_ids {
			MachineIds::Data(arr) => {
				if arr.len() > MAX_MACHINE_LIST_SIZE {
					vec![]
				}
				else {
					mac.dir.db.read_store_machines(&arr)?
				}
			},
			MachineIds::Unspecified => {
				mac.dir.db.read_store_machines_for_volume(volume_id)?
			}
		};

		let mut arr = macs.iter().filter(|m| {
			m.can_read()
		}).map(|ref m| {
			m.addr_ip.clone() + &m.addr_port.to_string()
		}).collect::<Vec<String>>();

		// Randomly choose any of the backends
		let mut rng = thread_rng();
		arr.shuffle(&mut rng);

		Ok(arr)
	};

	match store {

		StorePath::Needle { volume_id, key, alt_key, cookie } => {

			match parts.method {
				// Fetching a specific needle
				Method::GET => {
					
					let res = mac_handle.lock().unwrap().memory.lookup(NeedleKeys { key, alt_key });

					match res {
						Some(e) => respond_with_memory_entry(parts, cookie, &e),
						None => {
							let addrs = get_backend_addrs(&mac_handle.lock().unwrap(), volume_id)?;
											
							// Ideally I would just respond with it without retunring
							await!(respond_from_backend(
								parts, mac_handle, addrs, store.to_string(),
								volume_id, key, alt_key, cookie
							))
						}
					}
				},
				Method::POST => {
					// TODO: Performing a proxied upload to one or more store machines
					Ok(bad_request())
				},
				_ => Ok(bad_request())
			}
		},
		_ => Ok(bad_request())
	}
}

use bytes::Bytes;

#[async]
fn respond_from_backend(
	parts: Parts, mac_handle: MachineHandle, addrs: Vec<String>, store_path: String,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey, cookie: CookieBuf
) -> Result<Response<Body>> {

	let client = reqwest::async::Client::new();

	// TODO: Need to support streaming back a response as we get it from the store while we are putting it into the cache

	for addr in addrs {
		let res = await!(client.get(&format!("http://{}{}", addr, store_path)).send()).unwrap();


		// NOTE: Aside from general errors and corruption, we should be able to use the responses from any store
		if res.status().is_server_error() {
			continue;
		}

		if res.status() == StatusCode::OK {
			let mut headers = HeaderMap::new();

			for (name, value) in res.headers().iter() {
				let norm = name.to_string().to_lowercase();

				if norm.starts_with("x-haystack-") || &norm == "etag" {
					headers.insert(name, value.clone());
				} 
			}

			let content_length = res.headers().get("Content-Length").unwrap_or(
				&reqwest::header::HeaderValue::from(0)
			).to_str().unwrap_or("0").parse::<usize>().unwrap_or(0);
			// TODO: body.content_length() seems to be private?

			let body = res.into_body();

			let mut buf = Bytes::with_capacity(content_length as usize);
			
			#[async]
			for c in body {
				buf.extend_from_slice(&c);
			}

			let entry = MemoryEntry {
				logical_id: volume_id,
				cookie: cookie.clone(),
				headers,
				data: buf
			};

			let mac = mac_handle.lock().unwrap();
			mac.memory.insert(NeedleKeys { key, alt_key }, entry);

			// XXX: Best to 
			return respond_with_memory_entry(parts, cookie, &entry);
		}
		else {
			// TODO: Headers as well
			return Ok(Response::builder().status(res.status())
				.body("TODO".into()) // Body::from(res)) //.into_body().into())
				.unwrap());
		}
	}

	Ok(text_response(StatusCode::SERVICE_UNAVAILABLE, "No backend store able to respond"))

}

fn respond_with_memory_entry(
	parts: Parts, given_cookie: CookieBuf, entry: &MemoryEntry
) -> Result<Response<Body>> {

	if entry.cookie.data() != given_cookie.data() {
		// TODO: Keep in sync with the responses we use for the store
		return Ok(text_response(StatusCode::FORBIDDEN, "Incorrect cookie"))
	}

	// TODO: Handle If-None-Match, Range, etc.

	let mut res = Response::builder();

	res.status(StatusCode::OK); 

	for (name, value) in entry.headers.iter() {
		res.header(name, value.clone());
	}

	// TODO: Ensure this is zero copy
	// We should probably be passing this out
	Ok(res.body(Body::from(entry.data)).unwrap())
}

// TODO: It would be more efficient if we were to provide the list of machines as part of the query as the person requesting this would have to include those anyway

