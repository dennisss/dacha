#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants)]

extern crate haystack;
#[macro_use]
extern crate macros;

use std::fs::File;
use std::io::Read;

use common::errors::*;

use haystack::client::*;
use haystack::directory::Directory;
use haystack::types::*;
use haystack::Config;
use protobuf::Message;

#[derive(Args)]
struct Args {
    #[arg(
        short = "c",
        value_name = "CONFIG_FILE",
        desc = "Path to a yaml config file describing the setup of each component"
    )]
    config: Option<String>,

    command: ArgsCommand,
}

#[derive(Args)]
enum ArgsCommand {
    #[arg(name = "store", desc = "Start a store layer machine")]
    Store(StoreCommand),

    #[arg(name = "cache", desc = "Starts an intermediate caching layer machine")]
    Cache(CacheCommand),

    #[arg(
        name = "client",
        desc = "CLI Interface for interacting with a running haystack system made of the other commands"
    )]
    Client(ClientCommand),
}

#[derive(Args)]
struct StoreCommand {
    #[arg(
        desc = "Sets the listening http port",
        short = "p",
        value_name = "PORT",
        default = 4000
    )]
    port: u16,

    #[arg(
        desc = "Sets the data directory for store volumes",
        short = "f",
        value_name = "FOLDER",
        default = "/hay"
    )]
    folder: String,
}

#[derive(Args)]
struct CacheCommand {
    #[arg(
        desc = "Sets the listening http port",
        short = "p",
        value_name = "PORT",
        default = 4001
    )]
    port: u16,
}

#[derive(Args)]
enum ClientCommand {
    #[arg(name = "upload")]
    Upload(ClientUploadCommand),

    #[arg(name = "read-url")]
    ReadUrl(ClientReadUrlCommand),
}

#[derive(Args)]
struct ClientUploadCommand {
    #[arg(
        name = "ALT_KEY",
        positional,
        desc = "Alternative key integer to use for this upload"
    )]
    alt_key: NeedleAltKey,

    #[arg(
        name = "INPUT_FILE",
        positional,
        desc = "Path to the file to be uploaded"
    )]
    input_file: String,
}

#[derive(Args)]
struct ClientReadUrlCommand {
    #[arg(name = "KEY", positional)]
    key: NeedleKey,

    #[arg(name = "ALT_KEY", positional)]
    alt_key: NeedleAltKey,
}

#[executor_main]
async fn main() -> Result<()> {
    // let matches = App::new("Haystack")
    // 	.about("Photo/object storage system")

    // TODO: Would also be useful to print out a default config file so that it can
    // then be edited nicely

    let args = common::args::parse_args::<Args>()?;

    let mut config = Config::recommended();
    if let Some(config_file) = args.config {
        let mut file = File::open(config_file).expect("Failed to open the specified config file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        config
            .parse_merge(contents.as_bytes())
            .expect("Invalid config file");
    }

    let dir = Directory::open(config)?;

    // TODO: Will also eventually also have the pitch-fork
    match args.command {
        ArgsCommand::Store(cmd) => {
            haystack::store::main::run(dir, cmd.port, &cmd.folder).await?;
        }
        ArgsCommand::Cache(cmd) => {
            haystack::cache::main::run(dir, cmd.port).await?;
        }
        ArgsCommand::Client(cmd) => {
            let c = haystack::client::Client::create(dir);

            match cmd {
                ClientCommand::Upload(cmd) => {
                    println!("Starting upload");

                    let mut f = File::open(cmd.input_file)?;
                    let mut data = vec![];
                    f.read_to_end(&mut data)?;

                    let chunks = vec![PhotoChunk {
                        alt_key: cmd.alt_key,
                        data: data.into(),
                    }];

                    let pid = c.upload_photo(chunks).await?;
                    println!("Uploaded with photo id: {}", pid);
                }
                ClientCommand::ReadUrl(cmd) => {
                    let url = c
                        .read_photo_cache_url(&NeedleKeys {
                            key: cmd.key,
                            alt_key: cmd.alt_key,
                        })
                        .await?;
                    println!("{}", url);
                }
            }
        }
    }

    Ok(())
}
