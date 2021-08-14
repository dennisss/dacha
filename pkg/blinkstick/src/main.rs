#[macro_use]
extern crate common;
extern crate usb;

use common::errors::*;
use usb::hid::HIDDevice;

// sudo cp pkg/blinkstick/83-blinkstick.rules /etc/udev/rules.d/
// sudo udevadm control --reload-rules

const USB_IFACE: u8 = 0;
const USB_EP_IN: u8 = 0x81;


/// Setting this report changes the color of the first LED in the chain.
/// Payload: 3 bytes : [R, G, B]
/// Type: Feature
const FIRST_COLOR_REPORT_ID: u8 = 1;

/// Sets the BlinkStick Pro mode.
/// Payload: 1 byte
const MODE_SELECT_REPORT_ID: u8 = 4;

/// Set a single color at a given (channel, index).
/// Payload: 5 bytes: [channel_index, led_index, R, G, B]
/// Type: Feature
const SPECIFIC_COLOR_REPORT_ID: u8 = 5;

/// Sets the color values of the first N LEDs in a channel.
/// Payload: 1 + 3*N bytes : [channel_id] + N*[B, G, R]
/// - If less than N colors are given, the payload should be padded with zeros
///
/// For this report N=8.
const COLOR_DATA_8_REPORT_ID: u8 = 6;

/// Same as above, except N=16.
const COLOR_DATA_16_REPORT_ID: u8 = 6;

/// Same as above, except N=32.
const COLOR_DATA_32_REPORT_ID: u8 = 6;

/// Same as above, except N=64.
const COLOR_DATA_64_REPORT_ID: u8 = 6;

/*
Report ID: 1   : 8 x 3
    Send [r, g, b]

Report ID: 2   : 8 x 32
Report ID: 3   : 8 x 32
Report ID: 4   : 8 x 1
    Mode select for bitstick pro
Report ID: 5   : 8 x 5
    Send [channel, index, r, g, b]
Report ID: 6   : 8 x 25
    format is [channel, [g, r, b]*] (pad with zeros to the max num leds)
    8 leds
Report ID: 7   : 8 x 49
    16 leds
Report ID: 8   : 8 x 97
    32 leds
Report ID: 9   : 8 x 193
    64 leds
*/
pub struct BlinkStick {
    hid: HIDDevice
}

impl BlinkStick {
    pub async fn open() -> Result<Self> {
        let context = usb::Context::create()?;
        let device = context.open_device(0x0922, 0x1001).await?;
        let hid = HIDDevice::open_with_existing(device).await?;

        Ok(Self { hid })
    }

    pub async fn set_first_color(&self, color: RGB) -> Result<()> {
        self.hid.set_report(FIRST_COLOR_REPORT_ID, usb::hid::ReportType::Feature, 
            &[color.r, color.g, color.b]).await
    }

    pub async fn set_color(&self, channel: u8, index: u8, color: RGB) -> Result<()> {
        self.hid.set_report(SPECIFIC_COLOR_REPORT_ID, usb::hid::ReportType::Feature, 
            &[channel, index, color.r, color.g, color.b]).await
    }

    pub async fn set_colors(&self, channel: u8, colors: &[RGB]) -> Result<()> {
        let (report_id, num_colors) = match colors.len() {
            0..=8 => (COLOR_DATA_8_REPORT_ID, 8),
            9..=16 => (COLOR_DATA_16_REPORT_ID, 16),
            17..=32 => (COLOR_DATA_32_REPORT_ID, 32),
            33..=64 => (COLOR_DATA_64_REPORT_ID, 64),
            _ => {
                return Err(err_msg("Too many colors"));
            }
        };

        let mut payload = vec![0u8; 1 + 3*num_colors];
        payload[0] = channel;
        for (i, c) in colors.iter().enumerate() {
            payload[1 + 3*i] = c.g;
            payload[2 + 3*i] = c.r;
            payload[3 + 3*i] = c.b; 
        }

        // TODO: Ignore stalls.
        if let Err(e) = self.hid.set_report(report_id, usb::hid::ReportType::Feature, 
            &payload).await {
            if let Some(usb_e) = e.downcast_ref::<usb::Error>() {
                if usb_e.kind == usb::ErrorKind::TransferStalled {
                    return Ok(());
                }
            }

            return Err(e);
        }

        Ok(())
    }

    pub async fn turn_off(&self) -> Result<()> {


        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8
}


async fn read_controller() -> Result<()> {

    let blink = BlinkStick::open().await?;

    return Ok(());

    let x= 50;
    let c1 = RGB { r: 0, g: 0, b: x };
    let c2 = RGB { r: 0, g: x, b: 0 };

    loop {
        blink.set_colors(0, &[ c1, c2 ]).await?;

        common::wait_for(std::time::Duration::from_millis(500)).await;

        blink.set_colors(0, &[ c2, c1 ]).await?;

        common::wait_for(std::time::Duration::from_millis(500)).await;
    }

    // 0x03, 
    // parse_items(&[0x08, 0xf0])?;
    // return Ok(());



    /*
    let languages = device.read_languages().await?;
    println!("Product: {}", device.read_product_string(languages[0]).await?);

    let hid = HIDDevice::open_with_existing(device).await?;

    
*/




    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(read_controller())
}
