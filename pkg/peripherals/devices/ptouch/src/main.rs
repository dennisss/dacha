extern crate ptouch;
#[macro_use]
extern crate macros;

use std::time::Duration;

use base_error::*;
use image::{Color, Image};
use ptouch::*;

#[executor_main]
async fn main() -> Result<()> {
    let mut dev = LabelMaker::open().await?;

    dev.get_info().await?;
    let status = dev.get_status().await?;

    let tape = status.tape().ok_or_else(|| err_msg("No tape loaded"))?;

    println!("{:?}", tape);

    // dev.configure_settings().await?;

    Ok(())
}
