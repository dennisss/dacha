#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

extern crate haystack;
extern crate clap;
extern crate hyper;

use haystack::directory::Directory;
use haystack::cache::machine::CacheMachine;
use haystack::errors::*;
use std::sync::{Arc,Mutex};
use clap::{Arg, App};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};


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

	let machine = CacheMachine::load(dir, port)?;
	let mac_handle = Arc::new(Mutex::new(machine));

	let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);


	let on_start = || {
		//CacheMachine::start(&mac_handle);
	};

	start_http_server(
		port,
		&mac_handle,
		&haystack::cache::routes::handle_request,
		&on_start
	);

	Ok(())
}
