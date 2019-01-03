use super::super::common::*;
use super::super::errors::*;
use super::super::paths::*;
use super::machine::*;
use super::volume::*;
use super::needle::*;
use hyper::{Body, Request, Response, Method, StatusCode};
use hyper::http::request::Parts;
use hyper::body::Payload;
use mime_sniffer::MimeTypeSniffer;
use std::sync::{Arc,Mutex};
use futures::prelude::*;
use futures::future;
use futures::prelude::await;

pub type MachineHandle = Arc<Mutex<StoreMachine>>;


/// All requests will be of the form /<volume_id>/<key>/<alt_key>/<cookie>
/// NOTE: The error type doesn't really matter as we never resolve to a error, just as long as it is sendable across threads, hyper won't complain
pub fn handle_request(
	mac_handle: Arc<Mutex<StoreMachine>>, req: Request<Body>
) -> impl Future<Item=Response<Body>, Error=std::io::Error> {

	let (parts, body) = req.into_parts();

	// Mainly for being able to print out errors
	let method = parts.method.clone();
	let uri = parts.uri.clone();

	handle_request_inner(mac_handle, parts, body).then(move |res| {
		match res {
			Ok(resp) => Ok(resp),
			Err(e) => {
				println!("{} {}: {:?}", method, uri, e);
				Ok(Response::builder().status(500).body(Body::empty()).unwrap())
			}
		}
	})
}

#[async]
fn handle_request_inner(
	mac_handle: Arc<Mutex<StoreMachine>>, parts: Parts, body: Body
) -> Result<Response<Body>> {
	
	let segs = match split_path_segments(&parts.uri.path()) {
		Some(v) => v,
		None => return Ok(bad_request())
	};

	// We should not be getting any query parameters
	if parts.uri.query() != None {
		return Ok(bad_request());
	}

	let params = StorePath::from(&segs);
	match params {

		StorePath::Index => {
			match parts.method {
				Method::GET => index_volumes(mac_handle),
				_ => Ok(bad_request())
			}
		},

		StorePath::Volume { volume_id } => {
			match parts.method {
				Method::GET => read_volume(mac_handle, volume_id),
				Method::POST => create_volume(mac_handle, volume_id),
				_ => Ok(bad_request())
			}
		},

		StorePath::Photo { volume_id, key } => {
			Ok(bad_request())
		},

		StorePath::Partial { volume_id, key, alt_key } => {
			match parts.method {
				Method::GET => read_photo(mac_handle, volume_id, key, alt_key, None),
				_ => return Ok(bad_request())
			}
		},

		StorePath::Needle { volume_id, key, alt_key, cookie } => {
			match parts.method {
				Method::GET => read_photo(mac_handle, volume_id, key, alt_key, Some(cookie)),
				Method::POST => {
					
					/*
					let content_length_raw = match parts.headers.get("Content-Length") {
						Some(v) => v,
						None => return Ok(text_response(StatusCode::LENGTH_REQUIRED, "Missing Content-Length"))
					};

					let content_length = match content_length_raw.to_str().unwrap_or("-").parse::<u64>() {
						Ok(v) => v,
						Err(_) => return Ok(bad_request())
					};
					*/

					let content_length = match body.content_length() {
						Some(0) | None => {
							return Ok(text_response(StatusCode::LENGTH_REQUIRED, "Missing Content-Length"));
						},
						Some(n) => n,
					};

					await!(write_photo(mac_handle, volume_id, key, alt_key, cookie, content_length, body))
				},
				_ => return Ok(bad_request())
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
	let mac = mac_handle.lock().unwrap();

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
	let mac = mac_handle.lock().unwrap();

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

	let mut mac = mac_handle.lock().unwrap();

	if mac.volumes.contains_key(&volume_id) {
		return Ok(text_response(StatusCode::OK, "Volume already exists"));
	}

	mac.create_volume(volume_id)?;

	Ok(text_response(StatusCode::CREATED, "Volume created!"))
}





fn read_photo(
	mac_handle: MachineHandle,
	volume_id: VolumeId, key: NeedleKey, alt_key: NeedleAltKey,
	given_cookie: Option<CookieBuf>

) -> Result<Response<Body>> {

	let mac = mac_handle.lock().unwrap();

	let writeable = mac.can_write();

	let vol_handle = match mac.volumes.get(&volume_id) {
		Some(v) => v,
		None => return Ok(
			text_response(StatusCode::NOT_FOUND, "Volume not found")
		),
	};

	let mut vol = vol_handle.lock().unwrap();

	// TODO: I do want to be able to support exporting legit errors
	let r = vol.read_needle(&NeedleKeys { key, alt_key })?;

	let n = match r {
		Some(n) => n,
		None => {
			return Ok(
				text_response(StatusCode::NOT_FOUND, "Needle not found")
			)
		}
	};

	// Integrity check
	n.check()?;

	if let Some(c) = given_cookie {
		if c.data() != n.header.cookie.data() {
			return Ok(text_response(StatusCode::FORBIDDEN, "Incorrect cookie"));
		}
	}
	else {
		// Cookie was not given
		// If we are configured to require cookies, then this would be a mistake
	}

	

	// Now producing the respone

	let cookie = n.header.cookie.to_string();

	let sum = serialize_urlbase64(n.crc32c());

	let mut res = Response::builder();
	
	res.status(StatusCode::OK);

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

	// TODO: Construct an etag based on the machine_id/volume_id/offset, offset and volume_id
	// ^ Although using the crc32 would be naturally better for caching between hits to backup stores

	Ok(
		res
		.header("X-Haystack-Cookie", cookie)
		.header("X-Haystack-Hash", String::from("crc32c=") + &sum)
		.header("X-Haystack-Writeable", if writeable { "1" } else { "0" })
		.body(Body::from(n.bytes()))
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

	let mut mac = mac_handle.lock().unwrap();

	let vol_handle = match mac.volumes.get(&volume_id) {
		Some(v) => v,
		None => return Ok(text_response(StatusCode::NOT_FOUND, "Volume not found")),
	};

	let mut strm = super::stream::ChunkedStream::from(chunks);

	// We would now like this to broadcast out the future that it produces
	vol_handle.lock().unwrap().append_needle(
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




fn bad_request() -> Response<Body> {
	Response::builder().status(StatusCode::BAD_REQUEST).body(Body::empty()).unwrap()
}

fn json_response<T>(code: StatusCode, obj: &T) -> Response<Body> where T: serde::Serialize {
	let body = serde_json::to_string(obj).unwrap();
	Response::builder()
		.status(code)
		.header("Content-Type", "application/json; charset=utf-8")
		.body(Body::from(body))
		.unwrap()
}

fn text_response(code: StatusCode, text: &'static str) -> Response<Body> {
	Response::builder()
		.status(code)
		.header("Content-Type", "text/plain; charset=utf-8")
		.body(Body::from(text))
		.unwrap()
}
