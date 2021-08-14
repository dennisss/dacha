#[macro_use]
extern crate common;
extern crate blinkstick;

use std::time::Duration;

use common::errors::*;
use blinkstick::*;

async fn read_controller() -> Result<()> {

    let blink = BlinkStick::open().await?;

    let x= 50;
    let c1 = RGB { r: 0, g: 0, b: x };
    let c2 = RGB { r: 0, g: x, b: 0 };

    for i in 0..10 {
        blink.set_colors(0, &[ c1, c2 ]).await?;

        common::wait_for(Duration::from_millis(500)).await;

        blink.set_colors(0, &[ c2, c1 ]).await?;

        common::wait_for(Duration::from_millis(500)).await;
    }

    blink.turn_off().await?;


    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(read_controller())
}
