use common::errors::*;
use common::bytes::Bytes;
use mime_sniffer::MimeTypeSniffer;

use crate::types::*;
use crate::paths::*;
use crate::http_utils::*;
use super::api::*;
use super::volume::*;
use crate::proto::service::*;
use crate::store::machine::*;


pub async fn handle_request(
	req: http::Request, mac_handle: &MachineContext
) -> Result<http::Response> {

	// Because ip addresses and ports can change across restarts, we will always verify the request based on a standard hostname pattern derived by this machine's exact id	
	if !Host::Store(mac_handle.id).check_against(&req.head) {
		return Ok(bad_request_because("Incorrect/invalid host"));
	}

	let segs = match split_path_segments(req.head.uri.path.as_str()) {
		Some(v) => v,
		None => return Ok(bad_request_because("Invalid path given"))
	};

	// We should not be getting any query parameters
	if req.head.uri.query.is_some() {
		return Ok(bad_request_because("Not expecting query parameters"));
	}

	let params = match StorePath::from(&segs) {
		Ok(v) => v,
		Err(s) => return Ok(text_response(http::status_code::BAD_REQUEST, s))
	};

	// TODO: Need to verify no Content-Encoding present in requests?

	match params {

		StorePath::Index => {
			match req.head.method {
				http::Method::GET => index_volumes(mac_handle).await,
				http::Method::PATCH => super::route_write::write_batch(mac_handle, req.body).await,
				_ => Ok(invalid_method())
			}
		},

		StorePath::Volume { volume_id } => {
			match req.head.method {
				http::Method::GET => read_volume(mac_handle, volume_id).await,
				http::Method::POST => create_volume(mac_handle, volume_id).await,
				_ => Ok(invalid_method())
			}
		},

		StorePath::Photo { volume_id, key } => {
			Ok(bad_request())
		},

		StorePath::Partial { volume_id, key, alt_key } => {
			match req.head.method {
				http::Method::GET => read_photo(&req.head, mac_handle, volume_id, key, alt_key, None).await,
				_ => return Ok(invalid_method())
			}
		},

		StorePath::Needle { volume_id, key, alt_key, cookie } => {
			match req.head.method {
				http::Method::GET => read_photo(&req.head, mac_handle, volume_id, key, alt_key, Some(cookie)).await,
				http::Method::POST => {
					
					let content_length = match req.body.len() {
						Some(0) | None => {
							return Ok(text_response(http::status_code::LENGTH_REQUIRED, "Missing Content-Length"));
						},
						Some(n) => n as u64,
					};

					super::route_write::write_single(mac_handle, volume_id, key, alt_key, cookie, content_length, req.body).await
				},
				_ => return Ok(invalid_method())
			}
		},

		_ => Ok(bad_request())
	}
}

fn physical_volume_to_read_response(vol: &PhysicalVolume) -> StoreReadVolumeBody {
	let mut body = StoreReadVolumeBody::default();
	body.set_id(vol.superblock.volume_id);
	body.set_num_needles(vol.num_needles() as u64);
	body.set_used_space(vol.used_space());
	body
}

async fn index_volumes(
	mac_handle: &MachineContext
) -> Result<http::Response> {
	let mac = mac_handle.inst.read().await;

	let mut res = IndexVolumesResponse::default();

	for (_, v) in mac.volumes.iter() {
		let volume = v.lock().await;
		res.add_volumes(physical_volume_to_read_response(&*volume));
	}

	Ok(json_response(http::status_code::OK, &res))
}

async fn read_volume(
	mac_handle: &MachineContext,
	volume_id: VolumeId
) -> Result<http::Response> {
	
	// NOTE: WE only really need this for long enough to acquire 
	let mac = mac_handle.inst.read().await;

	match mac.volumes.get(&volume_id) {
		Some(v) =>  Ok(
			json_response(http::status_code::OK, &physical_volume_to_read_response(&*v.lock().await))
		),
		None => Ok(
			text_response(http::status_code::NOT_FOUND, "Volume not found")
		)
	}
}

async fn create_volume(
	mac_handle: &MachineContext,
	volume_id: VolumeId
) -> Result<http::Response> {

	let mut mac = mac_handle.inst.write().await;

	if mac.volumes.contains_key(&volume_id) {
		return Ok(text_response(http::status_code::OK, "Volume already exists"));
	}

	let stats = mac.stats().await;
	if !stats.can_allocate() {
		return Ok(text_response(http::status_code::BAD_REQUEST, "Can not currently allocate volumes"));
	}

	mac.create_volume(volume_id)?;

	println!("- Volume {} created on Store {}", volume_id, mac_handle.id);
	mac_handle.thread.notify();

	Ok(text_response(http::status_code::CREATED, "Volume created!"))
}




async fn read_photo(
	req_head: &http::RequestHead,
	mac_handle: &MachineContext,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey,
	given_cookie: Option<CookieBuf>
) -> Result<http::Response> {

	// Briefly lock the machine just to get the volume handle
	let vol_handle = {
		let mac = mac_handle.inst.read().await;

		let v = match mac.volumes.get(&volume_id) {
			Some(v) => v.clone(),
			None => return Ok(
				text_response(http::status_code::NOT_FOUND, "Volume not found")
			),
		};

		v
	};

	let mac_id = mac_handle.id;
	let writeable = mac_handle.is_writeable();

	let mut vol = vol_handle.lock().await;


	let given_etag = match req_head.headers.get_one("If-None-Match")? {
		Some(v) => {
			match ETag::from_header(v.value.as_bytes()) {
				Ok(e) => Some(e),
				Err(_) => return Ok(bad_request())
			}
		},
		None => None
	};

	// Under the condition that we are using a priveleged route (requesting without a cookie), we will allow checking based solely on the etag value (this will be used exclusively if the cache already has a potential value and just needs us to validate it)
	if given_cookie.is_none() {
		if let Some(ref e) = given_etag {
			let off = vol.peek_needle_block_offset(&NeedleKeys { key, alt_key });
			if let Some(offset) = off {
				if e.partial_matches(mac_id, volume_id, offset) {
					// TODO: This response must always be as close possible to the actual response we would give lower both in this function (- the body)
					return Ok(http::ResponseBuilder::new()
						.status(http::status_code::NOT_MODIFIED)
						.header("ETag", e.to_string()) // Reflecting back the cache given ETag
						.header("X-Haystack-Writeable", if writeable { "1" } else { "0" })
						.build()
						.unwrap());
				}
			}
		}
	}


	// TODO: I do want to be able to support exporting legit errors
	let r = vol.read_needle(&NeedleKeys { key, alt_key })?;

	let (n, offset) = match r {
		Some(n) => (n.needle, n.block_offset),
		None => {
			return Ok(
				text_response(http::status_code::NOT_FOUND, "Needle not found")
			)
		}
	};

	// Integrity check
	if let Err(_) = n.check() {
		return Ok(text_response(http::status_code::INTERNAL_SERVER_ERROR, "Integrity check failed"));
	}

	if let Some(c) = given_cookie {
		if c.data() != n.header.cookie.data() {
			return Ok(text_response(http::status_code::FORBIDDEN, "Incorrect cookie"));
		}
	}
	else {
		// Cookie was not given
		// NOTE: In some privileged cases, the cache will request the resource without the cookie
	}

	

	// Now producing the response

	let cookie = n.header.cookie.to_string();

	let sum = serialize_urlbase64(n.crc32c());

	let mut res = http::ResponseBuilder::new();
	
	// The etag is mainly designed to make hits to the same machine very efficient and hits to other machines at least able to notice after a disk read
	let etag = ETag {
		store_id: mac_id, volume_id, block_offset: offset, checksum: Bytes::from(n.crc32c())
	};	

	res = res
	.header("ETag", etag.to_string())
	.header("X-Haystack-Cookie", cookie)
	.header("X-Haystack-Hash", String::from("crc32c=") + &sum)
	.header("X-Haystack-Writeable", if writeable { "1" } else { "0" });

	if let Some(e) = given_etag {
		if etag.matches(&e) {
			return Ok(res.status(http::status_code::NOT_MODIFIED).build().unwrap());
		}
	}

	// Sniffing the Content-Type from the first few bytes of the file
	// For images, this should pretty much always work
	// TODO: If we were obsessed with performance, we would do this on the cache server to avoid transfering it
	{
		let data = n.data();
		if data.len() > 4 {
			let magic = &data[0..std::cmp::min(8, data.len())];
			res = match magic.sniff_mime_type() {
				Some(mime) => res.header("Content-Type", mime.to_owned()),
				None => res.header("Content-Type", "application/octet-stream")
			};
		}
	}


	Ok(
		res
		.status(http::status_code::OK)
		.body(http::BodyFromData(n.data_bytes()))
		.build()
		.unwrap()
	)
}

// Deletes a single photo needle
fn delete_photo(
	mac_handle: &MachineContext,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey
) -> Result<http::Response> {

	Ok(text_response(http::status_code::OK, "Woo!"))
}

// Deletes all photo needles associated with a single photo
fn delete_photo_all(
	mac_handle: &MachineContext,
	volume_id: VolumeId, key: NeedleKey
) -> Result<http::Response> {

	Ok(text_response(http::status_code::OK, "Woo!"))
}



