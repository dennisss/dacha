#[macro_use]
extern crate common;
extern crate crypto;
#[macro_use]
extern crate macros;
extern crate radix;

use common::errors::*;
use crypto::random::SharedRng;

async fn run() -> Result<()> {
    let rng = crypto::random::global_rng();

    let mut buf = [0u8; 16];
    rng.generate_bytes(&mut buf).await;

    println!("{}", radix::hex_encode(&buf[..]));

    // crypto::random::println!("Hello");

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
