#[macro_use]
extern crate common;
extern crate blinkstick;
extern crate math;
#[macro_use]
extern crate macros;

use std::{f32::consts::PI, time::Duration};

use blinkstick::*;
use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {
    println!("{}", -5.0 % 3.0);
    println!("{}", 5.0 % 3.0);
    println!("{}", 1.0 % 3.0);

    let blink = BlinkStick::open().await?;

    let x = 50;
    let c1 = RGB::new(0, 0, x);
    let c2 = RGB::new(0, x, 0);
    let c3 = RGB::new(x, 0, 0);

    for _ in 0..10 {
        blink
            .transition(c1, c2, Duration::from_millis(2000))
            .await?;
        blink
            .transition(c2, c3, Duration::from_millis(2000))
            .await?;
        blink
            .transition(c3, c1, Duration::from_millis(2000))
            .await?;

        /*
        blink.set_colors(0, &[ c1, c2 ]).await?;

        executor::sleep(Duration::from_millis(500)).await;

        blink.set_colors(0, &[ c2, c1 ]).await?;

        executor::sleep(Duration::from_millis(500)).await;
        */
    }

    blink.turn_off().await?;

    Ok(())
}
