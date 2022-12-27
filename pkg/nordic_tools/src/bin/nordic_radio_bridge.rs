// Server that interacts with a USB NRF52 dongle for communicating with remote
// NRF52 devices via this server's exposed RPC interface.

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use common::errors::*;

#[derive(Args)]
struct Args {
    /// Name of the object in the metastore to be used for storing the state of
    /// this bridge.
    state_object_name: String,

    rpc_port: rpc_util::NamedPortArg,

    usb: usb::DeviceSelector,
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    let bridge =
        nordic_tools::radio_bridge::RadioBridge::create(&args.state_object_name, &args.usb).await?;

    let mut task_bundle = executor::bundle::TaskResultBundle::new();

    let mut rpc_server = rpc::Http2Server::new();
    rpc_server.set_shutdown_token(executor::signals::new_shutdown_token());
    bridge.add_services(&mut rpc_server)?;
    task_bundle
        .add("rpc::Server", rpc_server.run(args.rpc_port.value()))
        .add("RadioBridge", bridge.run());

    task_bundle.join().await?;

    Ok(())
}

fn main() -> Result<()> {
    executor::run(run())?
}
