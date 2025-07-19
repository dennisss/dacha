#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    println!("Public IP: {:?}", public_ip::public_ip().await?);
    Ok(())
}