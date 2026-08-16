extern crate common;
extern crate net;
#[macro_use]
extern crate macros;

use std::string::ToString;
use std::time::Duration;

use common::errors::*;
use common::io::{Readable, Writeable};
use net::dns;
use net::tcp::{TcpListener, TcpStream};

#[executor_main]
async fn main() -> Result<()> {
    let mut dns = net::dns::Client::create_multicast_insecure().await?;

    let data = dns.resolve_ptr_many("_mocap._tcp.local.").await?;

    println!("{:#?}", data);

    Ok(())
}
