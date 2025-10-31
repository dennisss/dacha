#[macro_use]
extern crate macros;

use common::{errors::*, format::format_bytes};
use net::udev::UdevSocket;

#[derive(Args)]
struct Args {
    // addr: String,
}

/*
"ACTION": "remove",
"SUBSYSTEM": "usb",

"ACTION": "add",
"SUBSYSTEM": "usb",
*/

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let socket = UdevSocket::create()?;

    loop {
        let e = socket.recv().await?;
        println!("{:#?}", e);
    }

    Ok(())
}
