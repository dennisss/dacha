#[macro_use]
extern crate macros;

use common::{errors::*, format::format_bytes};
use net::udp::UdpBindOptions;

#[derive(Args)]
struct Args {
    addr: String,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let socket = net::udp::UdpSocket::bind_with_options(
        args.addr.parse()?,
        UdpBindOptions::new().reuse_addr(true).reuse_port(true),
    )
    .await?;

    let mut buf = vec![0u8; 1024];

    loop {
        let (n, addr) = socket.recv_from(&mut buf[..]).await?;
        println!("[{}] {}", addr.to_string(), format_bytes(&buf[0..n]));
    }

    Ok(())
}
