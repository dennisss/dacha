#[macro_use]
extern crate macros;

use std::time::Duration;

use common::errors::*;
use rpi::gpio::GPIOPin;
use rpi::{clock::*, gpio::GPIO, pwm::SysPWM};
use rpi::{pcm::*, ws2812};

/*

ssh -i ~/.ssh/id_cluster cluster-user@10.1.0.120

cargo build --target aarch64-unknown-linux-gnu --release --bin rpi

scp -i ~/.ssh/id_cluster target/aarch64-unknown-linux-gnu/release/rpi cluster-user@10.1.0.120:/home/cluster-user/rpi



*/

#[derive(Args)]
struct Args {
    duty_cycle: f32,
}

#[executor_main]
async fn main() -> Result<()> {
    let mut gpio = GPIO::open()?;

    {
        let mut pin = gpio.pin(4);
        pin.set_mode(rpi::gpio::Mode::Output);
        pin.set_resistor(rpi::gpio::Resistor::PullUp);
        pin.write(false);

        return Ok(());
    }

    Ok(())
}
