#![feature(async_await, futures_api, proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate diesel;
#[macro_use] extern crate error_chain;

extern crate futures;
// extern crate futures_await as futures;

extern crate core;
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
extern crate hyper;
extern crate ctrlc;
extern crate siphasher;
extern crate tokio;


pub mod errors {
	error_chain! {
		foreign_links {
			Io(::std::io::Error);
			Db(diesel::result::Error);
			HTTP(hyper::Error);
		}

		errors {
			// A type of error returned while performing a request
			// It is generally appropriate to respond with this text as a 400 error
			// We will eventually standardize the codes such that higher layers can easily distinguish errors
			API(code: u16, message: &'static str) {
				display("API Error: {} '{}'", code, message)
			}
		}
	}
}


macro_rules! enclose {
    ( ($( $x:ident ),*) $y:expr ) => {
        {
            $(let $x = $x.clone();)*
            $y
        }
    };
}


mod http;
pub mod common;
mod background_thread;
mod paths;
pub mod store;
pub mod directory;
pub mod cache;
pub mod client;

