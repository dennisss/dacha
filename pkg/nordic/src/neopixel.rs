/*
Neopixel protocol:
- Reset by staying low.
- 24 bits
    - Start high
        - 0.2 to 0.4us for a '0' code
            - Followed by at least 0.8us of low
        - 0.58 to 1.0us for a '1')
            - Followed by at least 0.2us of low
    - 80us of low is a reset

At 8MHz SPI, one bit is 0.125 us
- Use 3 bits for low,
- Use 6 bits for high
- Represent as two bytes with the second byte being always high
- so a 24-bit color requires 48 bytes to transfer

At 4Mhz SPI
- Each cycle is 0.25us
- 1 bit for low (less ideal than at 8Mhz but seems to work)
- 3 bits for high

*/

/*

TODO: Want to support inverted stuff:

SK6812MINI-E : 24-bit

IN-PI33QBTPRPGPBPW : 32-bit

*/


use common::fixed::vec::FixedVec;

use crate::spi::*;
use crate::pins::*;
use crate::gpio::*;
use crate::rtc::*;

pub struct Neopixel {
    spi: SPIHost,
    rtc: RTC,
}

impl Neopixel {
    pub fn new(periph: SPIMx, rtc: RTC, mut pin: GPIOPin) -> Self {
        pin
        .reset()
        .set_direction(PinDirection::Output)
        .write(PinLevel::Low);

        let spi = SPIHost::new::<_, DisconnectedPin, DisconnectedPin>(
            periph,
            4_000_000,
            Some(pin),
            None,
            None,
            None,
            SPIMode::Mode0,
        );

        Self { spi, rtc }
    }

    pub fn into_inner(self) -> SPIMx {
        self.spi.into_inner()
    }

    pub async fn write(&mut self, data: &[u8]) {
        self.rtc.wait_micros(100).await;

        let mut expanded = FixedVec::<u8, 256>::new();
        for byte in data {
            let expanded_byte = Self::expand_byte(*byte);
            expanded.extend_from_slice(&expanded_byte);
        }


        self.spi.transfer(&expanded, &mut []).await;

        self.rtc.wait_micros(100).await;
    }

    // NOTE: The data will be transfered MSB first.
    fn expand_byte(v: u8) -> [u8; 8] {
        let mut buf = [0u8; 8];

        for i in 0..8 {
            let bit = (v >> (7 - i)) & 1;
            buf[i] = {
                if bit != 0 {
                    0b11100000
                } else {
                    0b10000000
                }
            };
        }

        buf
    }
}

