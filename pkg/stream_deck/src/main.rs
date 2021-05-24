extern crate libusb;

#[macro_use]
extern crate common;

use common::errors::*;
use std::time::Duration;

type Result<T> = std::result::Result<T, Error>;

const USB_CONFIG: u8 = 1;
const USB_IFACE: u8 = 0;

// sudo cp pkg/stream_deck/81-elgato-stream-deck.rules /etc/udev/rules.d/
// sudo udevadm control --reload-rules

// TODO: Support the mic input

fn read_controller() -> Result<()> {
    let mut context = libusb::Context::new()?;

    let (mut device_handle, device_desc) = {
        let mut handle = None;

        for mut device in context.devices()?.iter() {
            let desc = device.device_descriptor()?;
            if desc.vendor_id() == 0x0fd9 && desc.product_id() == 0x006d {
                handle = Some((device.open()?, desc));
                break;
            }
        }

        handle.ok_or(err_msg("No device found"))?
    };

    let languages = device_handle.read_languages(Duration::from_secs(1))?;
    if languages.len() != 1 {
        return Err(err_msg("Expected only a single language"));
    }

    let product_name =
        device_handle.read_product_string(languages[0], &device_desc, Duration::from_secs(1))?;

    println!("Product name: {}", product_name);

    device_handle.reset()?;

    if device_handle.kernel_driver_active(USB_IFACE)? {
        println!("Detaching kernel driver.");
        device_handle.detach_kernel_driver(USB_IFACE)?;
    }

    device_handle.set_active_configuration(USB_CONFIG)?;
    device_handle.claim_interface(USB_IFACE)?;
    device_handle.set_alternate_setting(USB_IFACE, 0)?;

    let image = std::fs::read(project_path!("pkg/stream_deck/sample_icon.jpg"))?;

    // 0: 0x02
    // 1: 0x07
    // 2: 0x09 (Index of the button to change)
    // 3: 0x01 (1 if last packet, 0 otherwise)
    // 4-5: Little endian U16 packet length after the header (probably a u32 hence
    // why the rest of this is 0) 6-7: ? Usually zero

    // ATSAM9G45

    for btn_i in 0..4 {
        let mut remaining: &[u8] = &image[..];

        let mut packet_idx = 0;

        let mut buf = [0u8; 1024];
        while remaining.len() > 0 {
            let n = std::cmp::min(1024 - 8, remaining.len());

            buf[0] = 0x02;
            buf[1] = 0x07;
            buf[2] = btn_i;
            buf[3] = if n == remaining.len() { 1 } else { 0 };
            {
                let len = (n as u16).to_le_bytes();
                buf[4] = len[0];
                buf[5] = len[1];
            }
            {
                let val = (packet_idx as u16).to_le_bytes();
                buf[6] = val[0];
                buf[7] = val[1];
            }

            buf[8..(8 + n)].copy_from_slice(&remaining[0..n]);

            let nwritten = match device_handle.write_interrupt(0x02, &buf, Duration::new(1, 0)) {
                Err(libusb::Error::Timeout) => {
                    println!("Timed out writing packet");
                    return Ok(());
                }
                result @ _ => result?,
            };
            println!("Wrote: {}, {}", n, nwritten);

            remaining = &remaining[n..];
            packet_idx += 1;
        }
    }

    let mut buf = [0u8; 512];
    loop {
        let nread = match device_handle.read_interrupt(0x81, &mut buf, Duration::new(1, 0)) {
            Err(libusb::Error::Timeout) => {
                // println!("Timed out");
                continue;
            }
            result @ _ => result?,
        };

        println!("Read {:?}", &buf[0..20]);
    }

    println!("Opened!");

    /*
    let mut last_state = StadiaControllerState::default();

    let abs_max = std::u16::MAX as i32;
    let abs_min = abs_max * -1;

    // TODO: Need a constant GUID

    let mut buf = [0u8; 512];
    loop {
        let nread = match device_handle.read_interrupt(0x83, &mut buf, Duration::new(1, 0)) {
            Err(libusb::Error::Timeout) => {
                // println!("Timed out");
                continue;
            }
            result @ _ => result?,
        };

        // TODO: Remove this as it is in parse_usb_packet?

        let state = StadiaControllerState::parse_usb_packet(&buf[0..nread])?;
    }
    */

    Ok(())
}

fn main() -> Result<()> {
    println!("Hello!");

    read_controller()
}
