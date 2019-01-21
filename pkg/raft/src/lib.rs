#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate error_chain;

extern crate futures_await as futures;
extern crate rand;
extern crate serde;
extern crate rmp_serde as rmps;
extern crate hyper;
extern crate tokio;
extern crate bytes;
extern crate byteorder;
extern crate crc32c;


pub mod errors {
	error_chain! {
		foreign_links {
			Io(::std::io::Error);
			HTTP(hyper::Error);
		}
	}
}

pub mod sync;
pub mod atomic;

pub mod protos; // TODO: Eventually make this private again


pub mod log; // XXX: Likewise should be private
mod state;
pub mod snapshot;
pub mod consensus;

// Higher level complete implementation dealing with actual networking issues
pub mod rpc;
// XXX: Should only really be required by the server itself
pub mod state_machine;
pub mod server_protos;
pub mod server;
