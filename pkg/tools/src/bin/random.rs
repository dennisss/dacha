#[macro_use]
extern crate common;
extern crate crypto;
#[macro_use]
extern crate macros;
extern crate base_radix;

use common::errors::*;
use crypto::random::SharedRng;

#[executor_main]
async fn main() -> Result<()> {
    let rng = crypto::random::global_rng();

    let mut buf = [0u8; 16];
    rng.generate_bytes(&mut buf).await;

    println!("{}", base_radix::hex_encode(&buf[..]));

    // crypto::random::println!("Hello");

    Ok(())
}
