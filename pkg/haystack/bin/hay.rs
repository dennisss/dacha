#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

extern crate haystack;
extern crate clap;
extern crate futures;
extern crate toml;

use haystack::directory::Directory;
use haystack::errors::*;
use haystack::common::*;
use clap::{Arg, App, SubCommand};

use haystack::client::*;
use std::fs::File;
use std::io::{Read};
use futures::Future;



fn main() -> Result<()> {

	let matches = App::new("Haystack")
		.about("Photo/object storage system")
		.arg(Arg::with_name("config")
			.short("c")
			.long("config")
			.value_name("CONFIG_FILE")
			.help("Path to a yaml config file describing the setup of each component")
			.takes_value(true))
		.subcommand(
			SubCommand::with_name("store")
			.about("Start a store layer machine")
			.arg(Arg::with_name("port")
				.short("p")
				.long("port")
				.value_name("PORT")
				.help("Sets the listening http port")
				.takes_value(true))
			.arg(Arg::with_name("folder")
				.short("f")
				.long("folder")
				.value_name("FOLDER")
				.help("Sets the data directory for store volumes")
				.takes_value(true))
		)
		// TODO: Would also be useful to print out a default config file so that it can then be edited nicely
		.subcommand(
			SubCommand::with_name("cache")
			.about("Starts an intermediate caching layer machine")
			.arg(Arg::with_name("port")
				.short("p")
				.long("port")
				.value_name("PORT")
				.help("Sets the listening http port")
				.takes_value(true))
		)
		.subcommand(
			SubCommand::with_name("client")
			.about("CLI Interface for interacting with a running haystack system made of the other commands")
			.subcommand(
				SubCommand::with_name("upload")
				.arg(Arg::with_name("ALT_KEY")
					.help("Alternative key integer to use for this upload")
					.required(true)
					.index(1))
				.arg(Arg::with_name("INPUT_FILE")
					.help("Path to the file to be uploaded")
					.required(true)
					.index(2))
			)
			.subcommand(
				SubCommand::with_name("read-url")
				.arg(Arg::with_name("KEY").required(true).index(1))
				.arg(Arg::with_name("ALT_KEY").required(true).index(2))
			)
		)
		.get_matches();


	let config = if let Some(config_file) = matches.value_of("config") {
		let mut file = File::open(config_file).expect("Failed to open the specified config file");
		let mut contents = String::new();
		file.read_to_string(&mut contents)?;
		toml::from_str::<Config>(&contents).expect("Invalid config file")
	} else {
		Config::default()
	};

	let dir = Directory::open(config)?;

	match matches.subcommand() {
		("store", Some(m)) => {
			let port = m.value_of("port").unwrap_or("4000").parse::<u16>().expect("Invalid port given");
			let folder = m.value_of("folder").unwrap_or("/hay");
			haystack::store::main::run(dir, port, folder)?;
		},
		("cache", Some(m)) => {
			let port = m.value_of("port").unwrap_or("4001").parse::<u16>().expect("Invalid port given");
			haystack::cache::main::run(dir, port)?;
		},

		// TODO: Will also eventually also have the pitch-fork

		("client", Some(m)) => {

			let c = haystack::client::Client::create(dir);

			match m.subcommand() {
				("upload", Some(m)) => {
					println!("Starting upload");

					let alt_key = m.value_of("ALT_KEY").unwrap().parse::<NeedleAltKey>().unwrap();
					let filename = m.value_of("INPUT_FILE").unwrap();

					let mut f = File::open(filename)?;
					let mut data = vec![];
					f.read_to_end(&mut data)?;

					let chunks = vec![
						PhotoChunk {
							alt_key,
							data: data.into()
						}
					];

					let f = c.upload_photo(chunks)
					.map_err(|err| {
						println!("{:?}", err);
						()
					}).map(|pid| {
						println!("Uploaded with photo id: {}", pid);
						()
					});

					tokio::run(f);	

				},
				("read-url", Some(m)) => {
					let key = m.value_of("KEY").unwrap().parse::<NeedleKey>().unwrap();
					let alt_key = m.value_of("ALT_KEY").unwrap().parse::<NeedleAltKey>().unwrap();

					let url = c.read_photo_cache_url(&NeedleKeys {
						key, alt_key
					})?;

					println!("{}", url);
				},
				_ => return Err("Invalid subcommand".into())
			};
		},
		_ => return Err("Invalid subcommand".into())
	};

	Ok(())
}
