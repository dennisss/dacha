extern crate common;
extern crate ctrlc;
extern crate peripheral;
extern crate rpi;
extern crate stream_deck;

//  cross build --target=armv7-unknown-linux-gnueabihf --package home_hub
// scp target/armv7-unknown-linux-gnueabihf/debug/home_hub pi@10.1.0.44:~/

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use common::async_std::task;
use common::{errors::*, project_path};
use peripheral::ddc::DDCDevice;
use rpi::gpio::*;
use rpi::pwm::*;
use stream_deck::StreamDeckDevice;

/*
NOTE: Must use 72 x 72 non-progressive JPEGs
*/

#[derive(Debug)]
pub enum Event {
    KeyUp(usize),
    KeyDown(usize),
}

async fn run_stream_deck() -> Result<()> {
    let deck = StreamDeckDevice::open().await?;

    deck.set_display_timeout(60).await?;

    let computer_active =
        common::async_std::fs::read(project_path!("pkg/home_hub/icons/computer-active.jpg"))
            .await?;
    let computer_default =
        common::async_std::fs::read(project_path!("pkg/home_hub/icons/computer.jpg")).await?;

    let laptop_active =
        common::async_std::fs::read(project_path!("pkg/home_hub/icons/laptop-active.jpg")).await?;
    let laptop_default =
        common::async_std::fs::read(project_path!("pkg/home_hub/icons/laptop.jpg")).await?;

    let mut ddc = DDCDevice::open("/dev/i2c-20")?;

    // ddc.read_edid()?;

    // TODO: Maybe we should be resilient to the display possibly dying.

    let mut last_key_state = vec![];

    loop {
        let mut num_attempts = 0;

        let feature;
        loop {
            match ddc.get_vcp_feature(0x60) {
                Ok(f) => {
                    feature = f;
                    break;
                }
                Err(e) => {
                    num_attempts += 1;
                    if num_attempts == 10 {
                        return Err(e);
                    }

                    eprintln!("Failure getting feature: {}", e);

                    // TODO: Exponential backoff.
                    common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }

        let current_value = feature.current_value & 0xff;

        deck.set_key_image(
            0,
            if current_value == 0x0f {
                &computer_active
            } else {
                &computer_default
            },
        )
        .await?;
        deck.set_key_image(
            1,
            if current_value == 0x12 {
                &laptop_active
            } else {
                &laptop_default
            },
        )
        .await?;

        let key_state = deck.poll_key_state().await?;
        println!("GOT EVENTS");
        let mut events = vec![];
        for i in 0..key_state.len() {
            let old_value = last_key_state
                .get(i)
                .map(|v| *v)
                .unwrap_or(stream_deck::KeyState::Up);

            if old_value == key_state[i] {
                continue;
            } else if key_state[i] == stream_deck::KeyState::Up {
                events.push(Event::KeyUp(i));
            } else {
                events.push(Event::KeyDown(i));
            }
        }
        last_key_state = key_state;

        for event in events {
            println!("{:?}", event);
            match event {
                Event::KeyDown(0) => {
                    ddc.set_vcp_feature(0x60, 0x0F)?;
                }
                Event::KeyDown(1) => {
                    ddc.set_vcp_feature(0x60, 0x12)?;
                }
                _ => {}
            }
        }

        // task::sleep(Duration::from_secs(1)).await;
    }

    /*
    let caps = ddc.get_capabilities()?;
    println!("{}", caps);


    */

    /*
    // 0x0F is the Display Port 1

    // 0x12 is HDMI 2

    ddc.set_vcp_feature(0x60, 0x12)?;

    std::thread::sleep(std::time::Duration::from_secs(10));

    ddc.set_vcp_feature(0x60, 0x0f)?;
    */

    Ok(())
}

async fn run() -> Result<()> {
    run_stream_deck().await?;
    return Ok(());

    // let gpio = GPIO::open()?;

    // let pin = gpio.pin(12);

    // pin.set_mode(Mode::Input).set_mode(Mode::Output).write(false);

    let mut pwm = PWM::open()?;

    let mut cpu_temp_reader = rpi::temp::CPUTemperatureReader::create().await?;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    println!("Waiting for Ctrl-C...");
    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_secs(1));

        let temp = cpu_temp_reader.read().await?;

        println!("CPU Temp: {:.2}", temp);
    }

    println!("Exiting...");

    drop(pwm);

    // drop(pwm);

    Ok(())
}

fn main() -> Result<()> {
    task::block_on(run())
}
