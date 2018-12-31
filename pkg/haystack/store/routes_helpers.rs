
use std::io;
use std::io::{Cursor};
use rocket::http::{Status, ContentType};
use rocket::request::{Request};
use rocket::response::{Response, Responder};
use arrayref::*;
use super::common::*;
use super::needle::*;

// File mainly to store common http utilities


// Right now this is heavily biased towards the store

pub enum HaystackResponse {
	Ok(&'static str),
	Error(Status, &'static str),
	Json(String),
	Needle(HaystackNeedle)
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

				// TODO: Construct an etag based on the rcr, offset and volume_id

				// TODO: Also a header whether the machine is read-only (basically should be managed by the )

				Response::build()
					//.header(ContentType::PNG)
					.header(ContentType::Binary)
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
			Err(err) => {
				return rocket::request::Outcome::Failure((Status::BadRequest, "Invalid Content-Length"))
			}
		};

		rocket::request::Outcome::Success(ContentLength(num))
	}
}

/// Encapsulates something that may or may not look like a cookie
pub enum MaybeHaystackCookie {
	Data(Vec<u8>), // Basically should end 
	Invalid
}

impl MaybeHaystackCookie {	
	pub fn from(s: &str) -> MaybeHaystackCookie {
		let r = base64::decode_config(s, base64::URL_SAFE);
		match r {
			Ok(c) => {
				if c.len() != COOKIE_SIZE {
					return MaybeHaystackCookie::Invalid;
				}

				MaybeHaystackCookie::Data(c)
			},
			Err(_) => MaybeHaystackCookie::Invalid
		}
	}

	pub fn data(&self) -> Option<&[u8; COOKIE_SIZE]> {
		match self {
			MaybeHaystackCookie::Data(arr) => Some(array_ref!(arr, 0, COOKIE_SIZE)),
			MaybeHaystackCookie::Invalid => None
		}
	}
}


impl<'r> rocket::request::FromParam<'r> for MaybeHaystackCookie {
	type Error = &'r str;
	fn from_param(param: &'r rocket::http::RawStr) -> Result<Self, Self::Error> {
		Ok(MaybeHaystackCookie::from(param))
	}
}

impl<'v> rocket::request::FromFormValue<'v> for MaybeHaystackCookie {
	type Error = &'v str;
	fn from_form_value(form_value: &'v rocket::http::RawStr) -> Result<Self, Self::Error> {
		Ok(MaybeHaystackCookie::from(form_value))
	}
}

