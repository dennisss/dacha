#[macro_use]
extern crate common;
extern crate usb;
#[macro_use]
extern crate regexp_macros;

use common::errors::*;
use usb::hid::HIDDevice;

/// Setting this report changes the color of the first LED in the chain.
/// Payload: 3 bytes : [R, G, B]
/// Type: Feature
const FIRST_COLOR_REPORT_ID: u8 = 1;

const INFO_BLOCK1_REPORT_ID: u8 = 2;

const INFO_BLOCK2_REPORT_ID: u8 = 3;

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

const MAX_NUM_COLORS: usize = 64;

const MAX_NUM_CHANNELS: usize = 3;

regexp!(SERIAL_FORMAT => "^BS[0-9]+-([0-9]+)\\.([0-9]+)$");

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlinkStickVariant {
    Original,
    /// 3 x 64 LEDs (variable).
    Pro,
    /// 8 LEDs
    Square,
    /// 8 LEDs
    Strip,
    /// 2 LEDs
    Nano,
    /// 32 LEDs
    Flex,
}

/*
Report ID: 1   : 8 x 3
    Send [r, g, b]

Report ID: 2   : 8 x 32
    - Info Block 2
Report ID: 3   : 8 x 32
    - Info Block 3

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

/*
        // 10100000
        // 0x80 | 0x20

        // bmRequestType: 0x80 | 0x20,
        // bmRequest:  0x1,
        // wValue: 0x81,
        // wIndex: 0,
        // length: 2

0x81 is the led count report??
*/

pub struct BlinkStick {
    hid: HIDDevice,
    variant: BlinkStickVariant,
}

impl BlinkStick {
    pub async fn open() -> Result<Self> {
        let context = usb::Context::create()?;
        let device = context.open_device(0x20a0, 0x41e5).await?;
        let hid = HIDDevice::open_with_existing(device).await?;

        let languages = hid.device().read_languages().await?;
        if languages.len() != 1 {
            return Err(err_msg(
                "Expected usb device to have exactly one string language",
            ));
        }

        let serial = hid.device().read_serial_number_string(languages[0]).await?;

        let serial_match = SERIAL_FORMAT
            .exec(&serial)
            .ok_or_else(|| format_err!("Unrecognized BlinkStick serial format: {}", serial))?;

        let major_version = serial_match.group_str(1).unwrap()?.parse::<usize>()?;
        let minor_version = serial_match.group_str(2).unwrap()?.parse::<usize>()?;
        let bcd_device = hid.device().descriptor().bcdDevice;

        let variant = match (major_version, bcd_device) {
            (1, _) => BlinkStickVariant::Original,
            (2, _) => BlinkStickVariant::Pro,
            (3, 0x0200) => BlinkStickVariant::Square,
            (3, 0x0201) => BlinkStickVariant::Strip,
            (3, 0x0202) => BlinkStickVariant::Nano,
            (3, 0x0203) => BlinkStickVariant::Flex,
            _ => {
                return Err(err_msg("Unsupported BlinkStick variant"));
            }
        };

        println!("BlinkStick Variant: {:?}", variant);

        Ok(Self { hid, variant })
    }

    pub async fn set_first_color(&self, color: RGB) -> Result<()> {
        self.hid
            .set_report(
                FIRST_COLOR_REPORT_ID,
                usb::hid::ReportType::Feature,
                &[color.r, color.g, color.b],
            )
            .await
    }

    pub async fn set_color(&self, channel: u8, index: u8, color: RGB) -> Result<()> {
        self.hid
            .set_report(
                SPECIFIC_COLOR_REPORT_ID,
                usb::hid::ReportType::Feature,
                &[channel, index, color.r, color.g, color.b],
            )
            .await
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

        let mut payload = vec![0u8; 1 + 3 * num_colors];
        payload[0] = channel;
        for (i, c) in colors.iter().enumerate() {
            payload[1 + 3 * i] = c.g;
            payload[2 + 3 * i] = c.r;
            payload[3 + 3 * i] = c.b;
        }

        if let Err(e) = self
            .hid
            .set_report(report_id, usb::hid::ReportType::Feature, &payload)
            .await
        {
            // Ignore stalls as some devices don't have all the LEDs.
            if let Some(usb::Error::TransferStalled) = e.downcast_ref::<usb::Error>() {
                return Ok(());
            }

            return Err(e);
        }

        Ok(())
    }

    pub async fn turn_off(&self) -> Result<()> {
        for channel in 0..MAX_NUM_CHANNELS {
            self.set_colors(channel as u8, &[RGB { r: 0, g: 0, b: 0 }; MAX_NUM_COLORS])
                .await?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial() {
        let m = SERIAL_FORMAT.exec("BS040001-3.0").unwrap();
        assert_eq!(m.group_str(1).unwrap().unwrap(), "3");
        assert_eq!(m.group_str(2).unwrap().unwrap(), "0");
    }
}
