#[macro_use] extern crate common;
extern crate usb;

use std::time::Duration;

use common::errors::*;
use common::async_std::future::timeout;

const USB_CONFIG: u8 = 1;
const USB_IFACE: u8 = 0;

#[derive(PartialEq, Clone, Copy)]
pub enum KeyState {
    Up,
    Down
}

fn read_null_terminated_string(data: &[u8]) -> Result<String> {
    for i in 0..data.len() {
        if data[i] == 0x00 {
            return Ok(std::str::from_utf8(&data[0..i])?.to_string());
        }
    }

    Err(err_msg("Missing null terminator"))
}


pub struct StreamDeckDevice {
    device: usb::Device,
}

impl StreamDeckDevice {

    pub async fn open() -> Result<Self> {
        let context = usb::Context::create()?;

        let mut device = {
            let mut device = None;

            let entries = context.enumerate_devices().await?;
            for device_entry in entries {
                let device_desc = device_entry.device_descriptor()?;
                if device_desc.idVendor == 0x0fd9 && device_desc.idProduct == 0x006d {
                    device = Some(device_entry.open().await?);
                }
            }

            device.ok_or(err_msg("No device found"))?
        };

        // TODO: Set 1 second timeout
        let languages = device.read_languages().await?;
        if languages.len() != 1 {
            return Err(err_msg("Expected only a single language"));
        }

        println!("Languages: {:?}", languages);

        // TODO: Set 1 second timeout
        let product_name = device.read_product_string(languages[0]).await?;
        println!("Product name: {}", product_name);

        device.reset()?;
        
        if device.kernel_driver_active(USB_IFACE)? {
            println!("Detaching kernel driver.");
            device.detach_kernel_driver(USB_IFACE)?;
        }


        device.set_active_configuration(USB_CONFIG)?;
        device.claim_interface(USB_IFACE)?;
        device.set_alternate_setting(USB_IFACE, 0)?;

        /*

        

        // let mut report = vec![
        //     0x03, 0x08, 0x4e, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00,
        //     0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        //     0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x31, 0x8e,
        //     0x57, 0x84, 0x01, 0x00, 0x00
        // ];

        */


        Ok(Self { device })
    }

    /*
    Report Type: 0x08: Brightness

    Firmware Version: 0x05
    Serial Number 0x06

    */
    
    // TODO: Change all of these to use 

    pub async fn get_serial_number(&self) -> Result<String> {
        let mut report = vec![0u8; 32];

        self.device.read_control(usb::descriptors::SetupPacket {
            bmRequestType: 0xA1,
            bRequest: usb::hid::HIDRequestType::GET_REPORT.to_value(),
            wValue: 0x0306,
            wIndex: 0,
            wLength: report.len() as u16,
        }, &mut report).await?;

        read_null_terminated_string(&report[2..])
    }

    pub async fn get_firmware_number(&self) -> Result<String> {
        let mut report = vec![0u8; 32];

        self.device.read_control(usb::descriptors::SetupPacket {
            bmRequestType: 0xA1,
            bRequest: usb::hid::HIDRequestType::GET_REPORT.to_value(),
            wValue: 0x0305,
            wIndex: 0,
            wLength: report.len() as u16,
        }, &mut report).await?;

        read_null_terminated_string(&report[6..])
    }

    pub async fn set_brightness(&self, value: u8) -> Result<()> {
        self.device.write_control(usb::descriptors::SetupPacket {
            bmRequestType: 0x21,
            bRequest: usb::hid::HIDRequestType::SET_REPORT.to_value(),
            wValue: 0x0303,
            wIndex: 0,
            wLength: 3 as u16,
        }, &[ 0x03, 0x08, value ]).await
    }

    pub async fn set_display_timeout(&self, seconds: usize) -> Result<()> {
        let mut data = [0u8; 6];
        data[0] = 0x03;
        data[1] = 0x0d;
        *array_mut_ref![data, 2, 4] = (seconds as u32).to_le_bytes();
        
        self.device.write_control(usb::descriptors::SetupPacket {
            bmRequestType: 0x21,
            bRequest: usb::hid::HIDRequestType::SET_REPORT.to_value(),
            wValue: 0x0303,
            wIndex: 0,
            wLength: 6 as u16,
        }, &data).await
    }

    /// May return a usb::Error::Timeout.
    pub async fn set_key_image(&self, index: usize, image: &[u8]) -> Result<()> {
        let mut remaining: &[u8] = &image[..];

        let mut packet_idx = 0;

        // 1024 is the max packet size of the endpoint.
        let mut buf = [0u8; 1024];

        while remaining.len() > 0 {
            let n = std::cmp::min(buf.len() - 8, remaining.len());

            buf[0] = 0x02;
            buf[1] = 0x07;
            buf[2] = index as u8;
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


            common::async_std::future::timeout(
                Duration::from_secs(1),
                self.device.write_interrupt(0x02, &buf)).await??;

            remaining = &remaining[n..];
            packet_idx += 1;
        }

        Ok(())
    }

    pub async fn poll_key_state(&self) -> Result<Vec<KeyState>> {
        // 512 is the max packet size of the endpoint.
        let mut buf = [0u8; 512];

        // TODO: It's possible that 
        // When the first button is pressed 
        //           
        // [1, 0, 15, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        //               ^

        let nread = self.device.read_interrupt(0x81, &mut buf).await?;

        if nread < 4 {
            return Err(err_msg("Invalid key state packet"));
        }

        let unknown_number = u16::from_le_bytes(*array_ref![buf, 0, 2]) as usize;
        let num_keys = u16::from_le_bytes(*array_ref![buf, 2, 2]) as usize;

        if unknown_number != 1 {
            return Err(err_msg("Unsupported packet type"));
        }

        let key_data = &buf[4..nread];
        if key_data.len() < num_keys {
            return Err(err_msg("Invalid packet length"));
        }

        let mut out = vec![];
        for value in key_data.iter().cloned() {
            out.push(match value {
                0 => KeyState::Up,
                1 => KeyState::Down,
                _ => { return Err(err_msg("Invalid key state")); }
            });
        }

        Ok(out)
    }

}