#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;


use common::errors::*;

use klipper::*;

#[executor_main]
async fn main() -> Result<()> {

    KlipperDevice::create().await?;


    Ok(())
}
