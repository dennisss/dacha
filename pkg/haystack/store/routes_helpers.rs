
use std::io;
use std::io::{Cursor};
use rocket::http::{Status, ContentType};
use rocket::request::{Request};
use rocket::response::{Response, Responder};
use arrayref::*;
use mime_sniffer::MimeTypeSniffer;
use super::super::common::*;
use super::needle::*;

const COOKIE_SIZE: usize = std::mem::size_of::<Cookie>();


pub enum HaystackResponse {
	Ok(&'static str),
	Error(Status, &'static str),
	Json(String),
	Needle(Needle)
}

impl HaystackResponse {
	pub fn from<T>(obj: &T) -> HaystackResponse where T: serde::Serialize {
		let body = serde_json::to_string(obj).unwrap();
		HaystackResponse::Json(body)
	}
}

impl<'r> Responder<'r> for HaystackResponse {
	fn respond_to(self, _: &rocket::request::Request) -> rocket::response::Result<'r> {
		match self {
			HaystackResponse::Ok(msg) => {
				Response::build()
					.header(ContentType::Plain)
					.status(Status::Ok)
					.sized_body(Cursor::new(msg.to_owned())).ok()
			},
			HaystackResponse::Error(s, msg) => {
				Response::build()
					.header(ContentType::Plain)
					.sized_body(Cursor::new(msg.to_owned()))
					.status(s).ok()
			},
			HaystackResponse::Json(s) => {
				Response::build()
					.header(ContentType::JSON)
					.sized_body(Cursor::new(s))
					.status(Status::Ok).ok()
			},
			HaystackResponse::Needle(d) => {

				// TODO: Also export the checksum

				let mut cookie = base64::encode_config(&d.header.cookie, base64::URL_SAFE);
				cookie = cookie.trim_end_matches('=').to_string();

				let sum = base64::encode_config(d.crc32c(), base64::URL_SAFE);

				let mut res = Response::build();

				// Sniffing the Content-Type from the first few bytes of the file
				// For images, this should pretty much always work
				// TODO: If we were obsessed with performance, we would do this on the cache server to avoid transfering it
				{
					let data = d.data();
					if data.len() > 4 {
						let magic = &data[0..std::cmp::min(8, data.len())];
						match magic.sniff_mime_type() {
							Some(mime) => res.raw_header("Content-Type", mime.to_owned()),
							None => res.header(ContentType::Binary)
						};
					}
				}

				// TODO: Construct an etag based on the machine_id/volume_id/offset, offset and volume_id
				// ^ Although using the crc32 would be naturally better for caching between hits to backup stores

				// TODO: Also a header whether the machine is read-only (basically should be managed by the )

				res
					.raw_header("X-Haystack-Cookie", cookie)
					.raw_header("X-Haystack-Hash", String::from("crc32c=") + &sum)
					.sized_body(io::Cursor::new(d.bytes()))
					.ok()
			}
		}
    }
}

pub struct ContentLength(pub u64);

impl<'a, 'r> rocket::request::FromRequest<'a, 'r> for ContentLength {
    type Error = &'r str;

    fn from_request(request: &'a Request<'r>) -> rocket::request::Outcome<Self, Self::Error> {
		let s = match request.headers().get_one("Content-Length") {
			Some(s) => s,
			None => {
				return rocket::request::Outcome::Failure((Status::LengthRequired, "Missing Content-Length"))
			}
		};

		let num = match s.parse::<u64>() {
			Ok(n) => n,
			Err(_) => {
				return rocket::request::Outcome::Failure((Status::BadRequest, "Invalid Content-Length"))
			}
		};

		rocket::request::Outcome::Success(ContentLength(num))
	}
}

/// Encapsulates something that may or may not look like a cookie
pub enum MaybeCookie {
	Data(Vec<u8>), // Basically should end 
	Invalid
}

impl MaybeCookie {	
	pub fn from(s: &str) -> MaybeCookie {
		let r = base64::decode_config(s, base64::URL_SAFE);
		match r {
			Ok(c) => {
				if c.len() != COOKIE_SIZE {
					return MaybeCookie::Invalid;
				}

				MaybeCookie::Data(c)
			},
			Err(_) => MaybeCookie::Invalid
		}
	}

	pub fn data(&self) -> Option<&Cookie> {
		match self {
			MaybeCookie::Data(arr) => Some(array_ref!(arr, 0, COOKIE_SIZE)),
			MaybeCookie::Invalid => None
		}
	}
}


impl<'r> rocket::request::FromParam<'r> for MaybeCookie {
	type Error = &'r str;
	fn from_param(param: &'r rocket::http::RawStr) -> Result<Self, Self::Error> {
		Ok(MaybeCookie::from(param))
	}
}

impl<'v> rocket::request::FromFormValue<'v> for MaybeCookie {
	type Error = &'v str;
	fn from_form_value(form_value: &'v rocket::http::RawStr) -> Result<Self, Self::Error> {
		Ok(MaybeCookie::from(form_value))
	}
}

