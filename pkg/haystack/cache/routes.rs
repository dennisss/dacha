

use super::super::common::*;
use super::super::http::*;
use super::super::errors::*;
use super::super::directory;
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
use std::time::{SystemTime, Duration};
use rand::thread_rng;
use rand::seq::SliceRandom;
use reqwest::header::HeaderMap;
use futures::future;
use bytes::Bytes;


#[async]
pub fn handle_request(
	parts: Parts, body: Body, mac_handle: MachineHandle
) -> Result<Response<Body>> {

	let segs = match split_path_segments(&parts.uri.path()) {
		Some(v) => v,
		None => return Ok(bad_request_because("Not enough segments"))
	};

	// We should not be getting any query parameters
	if parts.uri.query() != None {
		return Ok(bad_request_because("Should not have given a query"));
	}

	let params = match CachePath::from(&segs) {
		Ok(v) => v,
		Err(s) => return Ok(bad_request_because(s))
	};

	match params {

		CachePath::Index => index_cache(mac_handle),

		CachePath::Proxy { machine_ids, store } => {
			await!(handle_proxy_request(parts, body, mac_handle, machine_ids, store))
		},


		_ => Ok(bad_request_because("Unsupported path pattern"))
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
	let mac = mac_handle.inst.lock().unwrap();

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


/// To mitigate backend DOS, this will limit the number of machines that can be specified as backends when making a request to the cache (does not apply in the unspecified mode)
const MAX_MACHINE_LIST_SIZE: usize = 6;

#[async]
fn handle_proxy_request(
	parts: Parts, body: Body, mac_handle: MachineHandle, machine_ids: MachineIds, store: StorePath
) -> Result<Response<Body>> {

	// Step one is to check in the cache for the pair (inclusive of the )
	// Check if an If-None-Match is given, etc.

	let mut store_str = store.to_string();

	// Will get the list of store machine addresses that for for this
	let get_backend_stores = move |mac: &CacheMachine, volume_id: VolumeId| -> Result<Vec<directory::models::StoreMachine>> {
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

		let mut arr = macs.into_iter().filter(|m| {
			m.can_read()
		}).collect::<Vec<_>>();

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
					let keys = NeedleKeys { key, alt_key };

					let mut store_macs;
					let mut old_entry;

					{ // Mutex scope
					
					let mut mac = mac_handle.inst.lock().unwrap();

					let res = mac.memory.lookup(&keys);

					if let Cached::Valid(ref e) = res {
						if e.logical_id == volume_id {
							return respond_with_memory_entry(parts, cookie, e.clone(), true);
						}
					}

					// TODO: Would be great to be able to do this without a lock on mac
					// This will currently bottle-neck our read-performance as we must hold the lock for this entire time
					store_macs = get_backend_stores(&mac, volume_id)?;

					old_entry = if let Cached::Stale(e) = res {

						// A malicious client could send bad cookies and evict entries that are stale from the cache prematurely (because we would end up requesting it with the store via the wrong cookie)
						// ^ Because we know that the cookie is immutable, we can verify the cookie right here and immediately put the entry back into the cache like nothing ever happened
						if e.cookie.data() != cookie.data() {
							mac.memory.insert(keys, e);
							return Ok(bad_request_because("Invalid cookie on stale entry"));
						}

						if let Some(idx) = store_macs.iter().position(|m| (m.id as MachineId) == e.store_id) {
							store_macs.swap(0, idx); // < Move as the first machine in the list
						}

						// Strip the cookie from the url that we send to the cache (that way we used the priveleged mode re-up etag check)
						store_str = (StorePath::Partial { volume_id, key, alt_key }).to_string();

						Some(e)
					} else {
						None
					};
					
					} // End mutex scope

					await!(respond_from_backend(
						parts, mac_handle, store_macs, store_str, old_entry,
						volume_id, key, alt_key, cookie
					))
				},
				Method::POST => {
					// TODO: Performing a proxied upload to one or more store machines
					Ok(bad_request_because("Not implemented"))
				},
				_ => Ok(bad_request_because("Invalid method"))
			}
		},
		_ => Ok(bad_request_because("Invalid store proxy route"))
	}
}


#[async]
fn respond_from_backend(
	parts: Parts, mac_handle: MachineHandle, store_macs: Vec<directory::models::StoreMachine>, store_path: String, old_entry: Option<Arc<MemoryEntry>>,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey, cookie: CookieBuf
) -> Result<Response<Body>> {

	// TODO: Make this more dynamic
	let from_cdn = false;

	// TODO: Next optimization would be to maintain the connections to the backends long term
	let client = reqwest::async::Client::new();

	// TODO: Need to support streaming back a response as we get it from the store while we are putting it into the cache

	for store_mac in store_macs {
		let route = format!("http://{}{}", store_mac.addr(), store_path);
		println!("sending to: {}", route);

		let probably_should_cache = !from_cdn && store_mac.can_write();

		let mut req = client.get(&route);
		req = req.header("Host", Host::Store(store_mac.id as MachineId).to_string());

		// In an optimization to not re-hit the stores on stale caches, we will attempt to reuse the etag
		// The backend store will recognize this by not reading from disk and not checking the cookie is the offsets in the etag are correct
		// NOTE: We do NOT try to passthrough any etag given by the client as our etags currently contain sensitive offset information and we don't want a client to be able to partially bypass the cookie check to sniff photo offsets in the store
		if let Some(ref e) = old_entry {
			if let Some(v) = e.headers.get("ETag") {
				req = req.header("If-None-Match", v);
			}

			// NOTE: In this case handle_proxy_request should have also stripped the store_path of the cookie
		}
		else if !probably_should_cache {
			// If this case we wil allow passing through the client etag (as we should still be forwarding the full store_path in this case)
			// NOTE: This is mainly so that we can operate in full-proxy mode for read-only stores
			if let Some(v) = parts.headers.get("If-None-Match") {
				req = req.header("If-None-Match", v);
			}
		}

		let res = match await!(req.send()) {
			Ok(r) => r,
			Err(e) => {
				eprintln!("Backend failed with {:?}", e);
				continue;
			}
		};


		// NOTE: Aside from general errors and corruption, we should be able to use the responses from any store
		if res.status().is_server_error() {
			continue;
		}

		if res.status() == StatusCode::OK || res.status() == StatusCode::NOT_MODIFIED {
			let mut headers = HeaderMap::new();

			for (name, value) in res.headers().iter() {
				let norm = name.to_string().to_lowercase();

				if norm.starts_with("x-haystack-") || &norm == "etag" {
					headers.insert(name, value.clone());
				} 
			}

			let mut buf: Bytes;

			// Regular case, get out the body
			if res.status() == StatusCode::OK {
				let content_length = res.headers().get("Content-Length").unwrap_or(
					&reqwest::header::HeaderValue::from(0)
				).to_str().unwrap_or("0").parse::<usize>().unwrap_or(0);
				// TODO: body.content_length() seems to be private?

				let body = res.into_body();

				buf = Bytes::with_capacity(content_length as usize);
				
				#[async]
				for c in body {
					buf.extend_from_slice(&c);
				}

			}
			// Otherwise we got a NotModified
			// This means that we had an old entry that we thought was stale and we can re-up it's lifetime
			else {
				// In this case, we used above the ETag from our cache, so we can merge the response with the one we already had in the cache
				if let Some(e) = old_entry {
					buf = e.data.clone();

					// Merge in old headers as the store may not have returned all of them again
					// (taking an old cached header only if not overriden by a new one)
					for (h, v) in e.headers.iter() {
						if let None = headers.get(h) {
							headers.insert(h, v.clone());
						}
					}
				}
				// Otherwise, we proxied the ETag from the client, so we will just reflect that back to them
				else {
					let mut res = Response::builder();
					res.status(StatusCode::NOT_MODIFIED);
					for (h, v) in headers.iter() {
						res.header(h, v.clone());
					}

					return Ok(res.body(Body::empty()).unwrap());
				}
			}

			let mut entry: Arc<MemoryEntry> = Arc::new(MemoryEntry {
				inserted_at: SystemTime::now(),
				store_id: store_mac.id as MachineId,
				logical_id: volume_id,
				cookie: cookie.clone(),
				headers,
				data: buf
			});

			let mut mac = mac_handle.inst.lock().unwrap();


			let is_writeable = if let Some(v) = entry.headers.get("X-Haystack-Writeable") {
				if v.to_str().unwrap_or("0") == "1" {
					true
				}
				else { false }
			} else { false };

			let should_cache = !from_cdn && is_writeable;

			// TODO: In the case of not-caching or the first response, we should be able to just stream back the body before we give the whole thing
			if should_cache {
				mac.memory.insert(NeedleKeys { key, alt_key }, entry.clone());
			}

			return respond_with_memory_entry(parts, cookie, entry, should_cache);
		}
		else {
			// TODO: Headers as well
			return Ok(Response::builder().status(res.status())
				// TODO: Passthrough store body
				.body(Body::empty()) // Body::from(res)) //.into_body().into())
				.unwrap());
		}
	}

	Ok(text_response(StatusCode::SERVICE_UNAVAILABLE, "No backend store able to respond"))

}

fn respond_with_memory_entry(
	parts: Parts, given_cookie: CookieBuf, entry: Arc<MemoryEntry>, will_cache: bool
) -> Result<Response<Body>> {

	if entry.cookie.data() != given_cookie.data() {
		// TODO: Keep in sync with the responses we use for the store
		return Ok(text_response(StatusCode::FORBIDDEN, "Incorrect cookie"))
	}


	// TODO: Implement Range, Expires headers (where the expires would be reflective of the internal cache state)

	let mut res = Response::builder();

	res.status(StatusCode::OK); 

	for (name, value) in entry.headers.iter() {
		res.header(name, value.clone());
	}

	// The Age header will only be on requests that we will actually store in memory
	if will_cache {
		let age = SystemTime::now().duration_since(entry.inserted_at)
			.unwrap_or(Duration::from_secs(0)).as_secs().to_string();

		res.header("Age", age);
	}

	// This is basically a long-winded way of checking if the client gave us a matching etag
	if let Some(v) = parts.headers.get("If-None-Match") {
		if let Some(v2) = entry.headers.get("ETag") {
			if let Ok(e) = ETag::from_header(v) {
				if let Ok(e2) = ETag::from_header(v2) {
					if e.matches(&e2) {
						return Ok(res.status(StatusCode::NOT_MODIFIED).body(Body::empty()).unwrap());
					}
				}
			}
		}
	}

	// TODO: Ensure this is zero copy
	// We should probably be passing this out
	Ok(
		res
		.status(StatusCode::OK)
		.body(Body::from(entry.data.clone())).unwrap()
	)
}

// TODO: It would be more efficient if we were to provide the list of machines as part of the query as the person requesting this would have to include those anyway

