use super::super::common::*;
use super::super::errors::*;
use super::super::paths::*;
use super::super::http::*;
use super::machine::*;
use super::volume::*;
use super::needle::*;
use hyper::{Body, Response, Method, StatusCode};
use hyper::http::request::Parts;
use hyper::body::Payload;
use mime_sniffer::MimeTypeSniffer;
use futures::prelude::*;
use futures::prelude::await;


#[async]
pub fn handle_request(
	parts: Parts, body: Body, mac_handle: MachineHandle
) -> Result<Response<Body>> {

	// Because ip addresses and ports can change across restarts, we will always verify the request based on a standard hostname pattern derived by this machine's exact id	
	if !Host::Store(mac_handle.id).check_against(&parts) {
		return Ok(bad_request_because("Incorrect/invalid host"));
	}

	let segs = match split_path_segments(&parts.uri.path()) {
		Some(v) => v,
		None => return Ok(bad_request_because("Invalid path given"))
	};

	// We should not be getting any query parameters
	if parts.uri.query() != None {
		return Ok(bad_request_because("Not expecting query parameters"));
	}

	let params = match StorePath::from(&segs) {
		Ok(v) => v,
		Err(s) => return Ok(text_response(StatusCode::BAD_REQUEST, s))
	};

	match params {

		StorePath::Index => {
			match parts.method {
				Method::GET => index_volumes(mac_handle),
				_ => Ok(invalid_method())
			}
		},

		StorePath::Volume { volume_id } => {
			match parts.method {
				Method::GET => read_volume(mac_handle, volume_id),
				Method::POST => create_volume(mac_handle, volume_id),
				_ => Ok(invalid_method())
			}
		},

		StorePath::Photo { volume_id, key } => {
			Ok(bad_request())
		},

		StorePath::Partial { volume_id, key, alt_key } => {
			match parts.method {
				Method::GET => read_photo(&parts, mac_handle, volume_id, key, alt_key, None),
				_ => return Ok(invalid_method())
			}
		},

		StorePath::Needle { volume_id, key, alt_key, cookie } => {
			match parts.method {
				Method::GET => read_photo(&parts, mac_handle, volume_id, key, alt_key, Some(cookie)),
				Method::POST => {
					
					let content_length = match body.content_length() {
						Some(0) | None => {
							return Ok(text_response(StatusCode::LENGTH_REQUIRED, "Missing Content-Length"));
						},
						Some(n) => n,
					};

					await!(write_photo(mac_handle, volume_id, key, alt_key, cookie, content_length, body))
				},
				_ => return Ok(invalid_method())
			}
		},

		_ => Ok(bad_request())
	}
}

#[derive(Serialize)]
struct StoreReadVolumeBody {
	id: VolumeId,
	num_needles: usize,
	used_space: usize
}

// TODO: 'std::convert::From<&PhysicalVolume> for'
impl StoreReadVolumeBody {
	fn from(vol: &PhysicalVolume) -> StoreReadVolumeBody {
		StoreReadVolumeBody {
			id: vol.volume_id,
			num_needles: vol.num_needles(),
			used_space: vol.used_space()
		}
	}
}

fn index_volumes(
	mac_handle: MachineHandle
) -> Result<Response<Body>> {
	let mac = mac_handle.inst.lock().unwrap();

	let mut arr: Vec<StoreReadVolumeBody> = vec![];

	for (_, v) in mac.volumes.iter() {
		arr.push(StoreReadVolumeBody::from(&v.lock().unwrap()));
	}

	Ok(json_response(StatusCode::OK, &arr))
}

fn read_volume(
	mac_handle: MachineHandle,
	volume_id: VolumeId
) -> Result<Response<Body>> {
	
	// NOTE: WE only really need this for long enough to acquire 
	let mac = mac_handle.inst.lock().unwrap();

	match mac.volumes.get(&volume_id) {
		Some(v) =>  Ok(
			json_response(StatusCode::OK, &StoreReadVolumeBody::from(&v.lock().unwrap()))
		),
		None => Ok(
			text_response(StatusCode::NOT_FOUND, "Volume not found")
		)
	}
}

fn create_volume(
	mac_handle: MachineHandle,
	volume_id: VolumeId
) -> Result<Response<Body>> {

	let mut mac = mac_handle.inst.lock().unwrap();

	if mac.volumes.contains_key(&volume_id) {
		return Ok(text_response(StatusCode::OK, "Volume already exists"));
	}

	if !mac.can_allocate() {
		return Ok(text_response(StatusCode::BAD_REQUEST, "Can not currently allocate volumes"));
	}

	mac.create_volume(volume_id)?;

	Ok(text_response(StatusCode::CREATED, "Volume created!"))
}




fn read_photo(
	parts: &Parts,
	mac_handle: MachineHandle,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey,
	given_cookie: Option<CookieBuf>

) -> Result<Response<Body>> {

	// Briefly lock the machine just to get some necessary info and a volume handle
	let (mac_id, writeable, vol_handle) = {
		let mac = mac_handle.inst.lock().unwrap();

		let v = match mac.volumes.get(&volume_id) {
			Some(v) => v.clone(),
			None => return Ok(
				text_response(StatusCode::NOT_FOUND, "Volume not found")
			),
		};

		// TODO: Current issue is that can_write is very expensive as it must lock all volumes
		(mac.id(), mac.can_write_soft(), v)
	};

	let mut vol = vol_handle.lock().unwrap();


	let given_etag = match parts.headers.get("If-None-Match") {
		Some(v) => {
			match ETag::from_header(v) {
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
					return Ok(Response::builder()
						.status(StatusCode::NOT_MODIFIED)
						.header("ETag", e.to_string()) // Reflecting back the cache given ETag
						.header("X-Haystack-Writeable", if writeable { "1" } else { "0" })
						.body(Body::empty()).unwrap());
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
				text_response(StatusCode::NOT_FOUND, "Needle not found")
			)
		}
	};

	// Integrity check
	if let Err(_) = n.check() {
		return Ok(text_response(StatusCode::INTERNAL_SERVER_ERROR, "Integrity check failed"));
	}

	if let Some(c) = given_cookie {
		if c.data() != n.header.cookie.data() {
			return Ok(text_response(StatusCode::FORBIDDEN, "Incorrect cookie"));
		}
	}
	else {
		// Cookie was not given
		// NOTE: In some privileged cases, the cache will request the resource without the cookie
	}

	

	// Now producing the response

	let cookie = n.header.cookie.to_string();

	let sum = serialize_urlbase64(n.crc32c());

	let mut res = Response::builder();
	
	// The etag is mainly designed to make hits to the same machine very efficient and hits to other machines at least able to notice after a disk read
	let etag = ETag {
		store_id: mac_id, volume_id, block_offset: offset, checksum: bytes::Bytes::from(n.crc32c())
	};	

	res
	.header("ETag", etag.to_string())
	.header("X-Haystack-Cookie", cookie)
	.header("X-Haystack-Hash", String::from("crc32c=") + &sum)
	.header("X-Haystack-Writeable", if writeable { "1" } else { "0" });

	if let Some(e) = given_etag {
		if etag.matches(&e) {
			return Ok(res.status(StatusCode::NOT_MODIFIED).body(Body::empty()).unwrap());
		}
	}

	// Sniffing the Content-Type from the first few bytes of the file
	// For images, this should pretty much always work
	// TODO: If we were obsessed with performance, we would do this on the cache server to avoid transfering it
	{
		let data = n.data();
		if data.len() > 4 {
			let magic = &data[0..std::cmp::min(8, data.len())];
			match magic.sniff_mime_type() {
				Some(mime) => res.header("Content-Type", mime.to_owned()),
				None => res.header("Content-Type", "application/octet-stream")
			};
		}
	}


	Ok(
		res
		.status(StatusCode::OK)
		.body(Body::from(n.data_bytes()))
		.unwrap()
	)
}

#[async]
fn write_photo(
	mac_handle: MachineHandle,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey, cookie: CookieBuf,
	content_length: u64,
	body: Body
) -> Result<Response<Body>> {

	let mut chunks = vec![];
	let mut nread = 0;

	#[async]
	for c in body {
		nread = nread + c.len();
		chunks.push(c.into_bytes());
		if nread >= (content_length as usize) {
			break;
		}
	}

	if nread != (content_length as usize) {
		return Ok(text_response(StatusCode::BAD_REQUEST, "Request payload bad length"));
	}

	let mut mac = mac_handle.inst.lock().unwrap();

	let vol_handle = match mac.volumes.get(&volume_id) {
		Some(v) => v,
		None => return Ok(text_response(StatusCode::NOT_FOUND, "Volume not found")),
	};

	let mut strm = super::stream::ChunkedStream::from(chunks);

	let mut vol = vol_handle.lock().unwrap();
	if !mac.can_write_volume(&vol) {
		return Ok(text_response(StatusCode::BAD_REQUEST, "Volume is out of space and not writeable"));
	}

	// We would now like this to broadcast out the future that it produces
	vol.append_needle(
		NeedleKeys { key, alt_key },
		cookie,
		NeedleMeta { flags: 0, size: content_length },
		&mut strm
	)?;

	Ok(text_response(StatusCode::OK, "Needle added!"))
}

// Deletes a single photo needle
fn delete_photo(
	mac_handle: MachineHandle,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey
) -> Result<Response<Body>> {

	Ok(text_response(StatusCode::OK, "Woo!"))
}

// Deletes all photo needles associated with a single photo
fn delete_photo_all(
	mac_handle: MachineHandle,
	volume_id: VolumeId, key: NeedleKey
) -> Result<Response<Body>> {

	Ok(text_response(StatusCode::OK, "Woo!"))
}


