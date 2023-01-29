extern crate common;
extern crate standing_desk;
#[macro_use]
extern crate macros;

use common::errors::*;

#[derive(Args)]
struct Args {
    device_name: String,
    radio_bridge_addr: String,
}

/*
cargo run --bin standing_desk -- --device_name=uplift_desk --radio_bridge_addr=127.0.0.1:8000
*/

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let client = standing_desk::Client::create(&args.radio_bridge_addr, &args.device_name).await?;
    // TODO: Wait for the Receive RPC to be registerd on the server.

    let sub = client.subscribe().await;

    client.query_state().await?;

    client.press_key(1).await?;

    loop {
        let mut packet = sub.recv().await?;
        println!("{:?}", packet);
    }

    Ok(())
}
