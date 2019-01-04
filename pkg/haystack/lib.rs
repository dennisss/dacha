#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate diesel;
#[macro_use] extern crate error_chain;


extern crate futures_await as futures;


extern crate dotenv;
extern crate crc32c;
extern crate rand;
extern crate byteorder;
extern crate arrayref;
extern crate bytes;
extern crate base64;
extern crate fs2;
extern crate serde;
extern crate serde_json;
extern crate mime_sniffer;
extern crate ipnetwork;
extern crate chrono;
extern crate bitwise;
extern crate hyper;
extern crate reqwest;

pub mod http;
pub mod common;
pub mod paths;
pub mod store;
pub mod directory;
pub mod cache;
pub mod client;


pub mod errors {
	error_chain! {
		foreign_links {
			Io(::std::io::Error);
			Db(diesel::result::Error);
			Client(reqwest::Error);
			Server(hyper::Error);
		}
	}
}