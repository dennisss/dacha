#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]
#[macro_use] extern crate rocket;
#[macro_use] extern crate serde_derive;
#[macro_use] extern crate diesel;
#[macro_use] extern crate error_chain;

extern crate dotenv;
extern crate crc32c;
extern crate rand;
extern crate byteorder;
extern crate arrayref;
extern crate futures;
extern crate bytes;
extern crate base64;
extern crate fs2;
extern crate serde;
extern crate serde_json;
extern crate mime_sniffer;
extern crate ipnetwork;
extern crate chrono;
extern crate bitwise;

pub mod common;
pub mod store;
pub mod directory;
pub mod client;


pub mod errors {
	error_chain! {
		foreign_links {
			Io(::std::io::Error);
			Db(diesel::result::Error);
		}
	}
}