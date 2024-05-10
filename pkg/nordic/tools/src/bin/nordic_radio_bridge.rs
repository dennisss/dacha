// Server that interacts with a USB NRF52 dongle for communicating with remote
// NRF52 devices via this server's exposed RPC interface.

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use common::errors::*;
use executor_multitask::RootResource;

#[derive(Args)]
struct Args {
    /// Name of the object in the metastore to be used for storing the state of
    /// this bridge.
    state_object_name: String,

    rpc_port: rpc_util::NamedPortArg,

    usb: usb::DeviceSelector,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    let bridge =
        nordic_tools::radio_bridge::RadioBridge::create(&args.state_object_name, &args.usb).await?;

    let service = RootResource::new();

    let mut rpc_server = rpc::Http2Server::new(Some(args.rpc_port.value()));
    bridge.add_services(&mut rpc_server)?;
    service.register_dependency(rpc_server.start()).await;

    service
        .spawn_interruptable("RadioBridge", bridge.run())
        .await;

    service.wait().await
}
