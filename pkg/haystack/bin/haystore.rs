#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]
#[macro_use] extern crate rocket;

extern crate haystack;
extern crate clap;

use rocket::http::{Status};
use rocket::config::{Config, Environment};
use rocket::fairing::AdHoc;
use haystack::store::routes_helpers::*;
use haystack::directory::Directory;
use haystack::store::machine::StoreMachine;
use haystack::errors::*;
use std::sync::{Arc,Mutex};
use clap::{Arg, App};


#[catch(404)]
fn not_found() -> HaystackResponse {
	HaystackResponse::Error(Status::BadRequest, "Invalid route")
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
	let mac_handle = Arc::new(Mutex::new(machine));

	let config = Config::build(Environment::Staging)
    .address("127.0.0.1")
    .port(port)
    .finalize().unwrap();

	rocket::custom(config)
	.mount("/", haystack::store::routes::get())
	.register(catchers![not_found])
	.manage(mac_handle.clone())
	.attach(AdHoc::on_launch("Store Ready", move |_| {
		StoreMachine::start(mac_handle);
	}))
	.launch();

	Ok(())
}
