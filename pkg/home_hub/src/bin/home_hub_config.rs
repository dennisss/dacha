#[macro_use]
extern crate common;
extern crate peripheral;
extern crate rpi;
extern crate stream_deck;
#[macro_use]
extern crate macros;
extern crate container;
extern crate home_hub;
extern crate hue;
extern crate protobuf;

use common::async_std::task;
use common::errors::*;
use home_hub::proto::config::Config;

#[derive(Args)]
struct Args {
    config_object: String,
    set_config: Option<String>,
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let meta_client = container::meta::client::ClusterMetaClient::create_from_environment().await?;

    if let Some(new_config) = args.set_config {
        let mut config = Config::default();
        protobuf::text::parse_text_proto(&new_config, &mut config)?;

        meta_client.set_object(&args.config_object, &config).await?;
    } else {
        let config = meta_client
            .get_object::<Config>(&args.config_object)
            .await?
            .ok_or_else(|| err_msg("No config set"))?;
        println!("{:?}", config);
    }

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}