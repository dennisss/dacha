#[macro_use]
extern crate common;
extern crate usb;

use std::time::Duration;

use common::errors::*;
use usb::hid::HIDDevice;

const USB_CONFIG: u8 = 1;
const USB_IFACE: u8 = 0;

const KEY_STATE_REPORT_ID: u8 = 1;

const KEY_IMAGE_REPORT_ID: u8 = 2;

const FIRMWARE_VERSION_REPORT_ID: u8 = 5;

const SERIAL_NUMBER_REPORT_ID: u8 = 6;

/*
Report 1: 8 x 511 (bit field) Input
- Key state

Report 2: 8 x 1023 (bitfield)  Output
- Key image

Report 3: 8 x 31 Feature (array)
- Display timeout + Brightness

Report 4: 8 x 31 Feature (array)

Report 5: 8 x 31 Feature (array)
- Firmware Version

Report 6: 8 x 31 Feature (array)
- Serial

Report 7: 8 x 31 Feature (array)

Report 8: 8 x 31 Feature (array)

Report 9: 8 x 31 Feature (array)

Report 10: 8 x 31 Feature (array)
*/

#[derive(PartialEq, Clone, Copy)]
pub enum KeyState {
    Up,
    Down,
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
    hid: HIDDevice,
}

impl StreamDeckDevice {
    pub async fn open() -> Result<Self> {
        let context = usb::Context::create()?;

        let device = context.open_device(0x0fd9, 0x006d).await?;

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

        let hid = HIDDevice::open_with_existing(device).await?;

        Ok(Self { hid })
    }

    pub async fn get_serial_number(&self) -> Result<String> {
        let mut report = vec![0u8; 31];
        self.hid
            .get_report(
                SERIAL_NUMBER_REPORT_ID,
                usb::hid::ReportType::Feature,
                &mut report,
            )
            .await?;
        read_null_terminated_string(&report[1..])
    }

    pub async fn get_firmware_number(&self) -> Result<String> {
        let mut report = vec![0u8; 31];
        self.hid
            .get_report(
                FIRMWARE_VERSION_REPORT_ID,
                usb::hid::ReportType::Feature,
                &mut report,
            )
            .await?;

        read_null_terminated_string(&report[5..])
    }

    pub async fn set_brightness(&self, value: u8) -> Result<()> {
        self.hid
            .set_report(3, usb::hid::ReportType::Feature, &[0x08, value])
            .await
    }

    pub async fn set_display_timeout(&self, seconds: usize) -> Result<()> {
        let mut data = [0u8; 5];
        data[0] = 0x0d;
        *array_mut_ref![data, 1, 4] = (seconds as u32).to_le_bytes();

        self.hid
            .set_report(3, usb::hid::ReportType::Feature, &data)
            .await?;

        Ok(())
    }

    /// May return a usb::Error::Timeout.
    pub async fn set_key_image(&self, index: usize, image: &[u8]) -> Result<()> {
        let mut remaining: &[u8] = &image[..];

        let mut packet_idx = 0;

        // 1024 is the max packet size of the endpoint.
        let mut buf = [0u8; 1023];

        while remaining.len() > 0 {
            let n = std::cmp::min(buf.len() - 7, remaining.len());

            buf[0] = 0x07;
            buf[1] = index as u8;
            buf[2] = if n == remaining.len() { 1 } else { 0 };
            {
                let len = (n as u16).to_le_bytes();
                buf[3] = len[0];
                buf[4] = len[1];
            }
            {
                let val = (packet_idx as u16).to_le_bytes();
                buf[5] = val[0];
                buf[6] = val[1];
            }

            buf[7..(7 + n)].copy_from_slice(&remaining[0..n]);

            common::async_std::future::timeout(
                Duration::from_secs(1),
                self.hid
                    .set_report(KEY_IMAGE_REPORT_ID, usb::hid::ReportType::Output, &buf),
            )
            .await??;

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

        let nread = self.hid.device().read_interrupt(0x81, &mut buf).await?;

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
                _ => {
                    return Err(err_msg("Invalid key state"));
                }
            });
        }

        Ok(out)
    }
}
