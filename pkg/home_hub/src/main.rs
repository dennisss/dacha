#[macro_use]
extern crate common;
extern crate peripheral;
extern crate rpi;
extern crate stream_deck;
#[macro_use]
extern crate macros;

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

#[derive(Args)]
struct Args {
    hdmi_ddc_device: String,
}

#[derive(Debug)]
pub enum Event {
    KeyUp(usize),
    KeyDown(usize),
}

const INPUT_SELECT_VCP_CODE: u8 = 0x60;

enum_def_with_unknown!(InputSelectValue u8 =>
    AnalogVideo1 = 0x01, // RGB 1
    AnalogVideo2 = 0x02, // RGB 2
    DigitalVideo1 = 0x03, // DVI 1
    DigitalVideo2 = 0x04, // DVI 2
    CompositeVideo1 = 0x05,
    CompositeVideo2 = 0x06,
    SVideo1 = 0x07,
    SVideo2 = 0x08,
    Tuner1 = 0x09,
    Tuner2 = 0x0A,
    Tuner3 = 0x0B,
    ComponentVideo1 = 0x0C,
    ComponentVideo2 = 0x0D,
    ComponentVideo3 = 0x0E,
    DisplayPort1 = 0x0F,
    DisplayPort2 = 0x10,
    HDMI1 = 0x11, // Digital Video 3
    HDMI2 = 0x12 // Digital Video 4
);

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

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

    let mut ddc = DDCDevice::open(&args.hdmi_ddc_device)?;

    // ddc.read_edid()?;

    // TODO: Maybe we should be resilient to the display possibly dying.

    let mut last_key_state = vec![];

    loop {
        let mut num_attempts = 0;

        let feature;
        loop {
            match ddc.get_vcp_feature(INPUT_SELECT_VCP_CODE) {
                Ok(f) => {
                    feature = f;
                    break;
                }
                Err(e) => {
                    num_attempts += 1;
                    if num_attempts == 60 {
                        return Err(e);
                    }

                    eprintln!("Failure getting feature (attempt {}): {}", num_attempts, e);

                    // TODO: Exponential backoff.
                    common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }

        let current_value = InputSelectValue::from_value((feature.current_value & 0xff) as u8);

        deck.set_key_image(
            0,
            if current_value == InputSelectValue::DisplayPort1 {
                &computer_active
            } else {
                &computer_default
            },
        )
        .await?;
        deck.set_key_image(
            1,
            if current_value == InputSelectValue::HDMI2 {
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
                    ddc.set_vcp_feature(
                        INPUT_SELECT_VCP_CODE,
                        InputSelectValue::DisplayPort1.to_value() as u16,
                    )?;
                }
                Event::KeyDown(1) => {
                    ddc.set_vcp_feature(
                        INPUT_SELECT_VCP_CODE,
                        InputSelectValue::HDMI2.to_value() as u16,
                    )?;
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

fn main() -> Result<()> {
    task::block_on(run())
}
