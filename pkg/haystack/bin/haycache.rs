#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

extern crate haystack;
extern crate clap;
extern crate hyper;

use haystack::directory::Directory;
use haystack::cache::machine::*;
use haystack::errors::*;
use haystack::http::start_http_server;
use std::sync::{Arc};
use clap::{Arg, App};
use std::{time, thread};

fn on_start(mac_handle: &MachineHandle) {
	CacheMachine::start(mac_handle);
}

fn on_stop(mac_handle: &MachineHandle) {
	mac_handle.thread.stop();

	// Wait for a small amount of time after we've been marked as not-ready in case stray requests are still pending
	let dur = time::Duration::from_millis(1000);
	thread::sleep(dur);
}

fn main() -> Result<()> {

	let matches = App::new("Haycache")
		.about("The intermediate caching layer")
		.arg(Arg::with_name("port")
			.short("p")
			.long("port")
			.value_name("PORT")
			.help("Sets the listening http port")
			.takes_value(true))
		.get_matches();

	let port = matches.value_of("port").unwrap_or("4001").parse::<u16>().expect("Invalid port given");

	let dir = Directory::open()?;

	// TODO: Whenever possible, re-use the ids of previously existing but now dead machines
	let machine = CacheMachine::load(dir, port)?;
	let mac_ctx = MachineContext::from(machine);

	let mac_handle = Arc::new(mac_ctx);

	start_http_server(
		port,
		&mac_handle,
		&haystack::cache::routes::handle_request,
		&on_start,
		&on_stop
	);

	Ok(())
}
