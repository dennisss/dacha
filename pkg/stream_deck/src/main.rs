
#[macro_use] extern crate common;
extern crate stream_deck;

use common::errors::*;


type Result<T> = std::result::Result<T, Error>;

// sudo cp pkg/stream_deck/81-elgato-stream-deck.rules /etc/udev/rules.d/
// sudo udevadm control --reload-rules

async fn read_controller() -> Result<()> {


    let image = std::fs::read(project_path!("pkg/stream_deck/sample_icon.jpg"))?;

    // 0: 0x02
    // 1: 0x07
    // 2: 0x09 (Index of the button to change)
    // 3: 0x01 (1 if last packet, 0 otherwise)
    // 4-5: Little endian U16 packet length after the header (probably a u32 hence
    // why the rest of this is 0) 6-7: ? Usually zero

    // ATSAM9G45

    let deck = stream_deck::StreamDeckDevice::open().await?;

    for btn_i in 0..4 {
        deck.set_key_image(btn_i, &image).await?;
    }

    deck.set_brightness(20).await?;
    deck.set_display_timeout(30).await?;
    
    println!("Serial Number: {}", deck.get_serial_number().await?);
    println!("Firmware Version: {}", deck.get_firmware_number().await?);

    loop {
        deck.poll_key_state().await?;
    }


    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(read_controller())
}
