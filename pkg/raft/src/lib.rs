#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate error_chain;

extern crate common;
extern crate crypto;
extern crate futures_await as futures;
extern crate rand;
extern crate serde;
extern crate rmp_serde as rmps;
extern crate hyper;
extern crate tokio;
extern crate bytes;
extern crate byteorder;


// TODO: Will be removed in favor of common::errors 
pub mod errors {
	error_chain! {
		foreign_links {
			Io(::std::io::Error);
			HTTP(hyper::Error);
			Utf8(std::str::Utf8Error);
			ParseInt(std::num::ParseIntError);
		}
	}
}

macro_rules! to_future {
    ($x:block) => ({
        match (move || $x)() {
			Ok(v) => ok(v),
			Err(e) => err(e)
        }
	})
}

macro_rules! to_future_box {
	($x:block) => ({
		Box::new(to_future!($x))
	});
}

pub mod sync;
pub mod atomic;

pub mod protos; // TODO: Eventually make this private again


pub mod log; // XXX: Likewise should be private
mod state;
//pub mod snapshot; // XXX: May eventually reoccur as a file that holds the algorithm for managing whether or not we should trigger snapshots
mod config_state;
pub mod constraint;
pub mod consensus;

//pub mod record_io;

// Higher level complete implementation dealing with actual networking issues
pub mod routing;
pub mod rpc;
// XXX: Should only really be required by the server itself
pub mod state_machine;
pub mod server_protos;
pub mod discovery;
pub mod server;

pub mod simple_log;
pub mod node;
