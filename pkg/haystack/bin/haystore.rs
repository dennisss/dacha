#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

extern crate haystack;
extern crate clap;

use haystack::directory::Directory;
use haystack::store::machine::*;
use haystack::errors::*;
use haystack::http::*;
use std::sync::Arc;
use clap::{Arg, App};
use std::{thread, time};


fn on_start(mac_handle: &MachineHandle) {
	StoreMachine::start(mac_handle);
}

fn on_stop(mac_handle: &MachineHandle) {
	mac_handle.thread.stop();

	// Wait for a small amount of time after we've been marked as not-ready in case stray requests are still pending
	let dur = time::Duration::from_millis(500);
	thread::sleep(dur);
}

fn main() -> Result<()> {

	let matches = App::new("Haystore")
		.about("The storage layer")
		.arg(Arg::with_name("port")
			.short("p")
			.long("port")
			.value_name("PORT")
			.help("Sets the listening http port")
			.takes_value(true))
		.arg(Arg::with_name("store")
			.short("f")
			.long("folder")
			.value_name("FOLDER")
			.help("Sets the data directory for store volumes")
			.takes_value(true))
		.get_matches();

	let port = matches.value_of("port").unwrap_or("4000").parse::<u16>().expect("Invalid port given");
	let store = matches.value_of("store").unwrap_or("/hay");

	let dir = Directory::open()?;

	let machine = StoreMachine::load(dir, port, store)?;
	println!("Starting Haystore Id #{}", machine.id());

	let mac_ctx = MachineContext::from(machine);

	let mac_handle = Arc::new(mac_ctx);


	start_http_server(
		port,
		&mac_handle,
		&haystack::store::routes::handle_request,
		&on_start,
		&on_stop
	);

	Ok(())
}
