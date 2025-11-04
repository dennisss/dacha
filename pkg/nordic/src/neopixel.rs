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
- GRB

IN-PI33QBTPRPGPBPW : 32-bit
- GRBW

*/


use common::fixed::vec::FixedVec;

use crate::spi::*;
use crate::pins::*;
use crate::gpio::*;
use crate::rtc::*;

pub struct Neopixel {
    spi: SPIHost,
    inverted: bool,
}

impl Neopixel {
    pub fn new(periph: SPIMx, mut pin: GPIOPin, inverted: bool) -> Self {
        // Note that for inverted mode, there is no 'simple' way to keep this
        // pin high when the SPI peripheral is inactive so we rely on creating
        // a reset period during the SPI transfer for inverted mode in 'write'.
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

        Self { spi, inverted }
    }

    pub fn into_inner(self) -> SPIMx {
        self.spi.into_inner()
    }

    pub async fn write(&mut self, data: &[u8]) {
        // TODO: Error out if we overflow this buffer.
        let mut expanded = FixedVec::<u8, 256>::new();

        let reset_byte = if self.inverted { 0xff } else { 0x00 };

        // 'RESET' : 40 bytes takes 80us to transfer
        // TODO: Might as well cache these in a global buffer (in which we only change the real data per run).
        for _ in 0..40 {
            expanded.push(reset_byte);
        }

        for byte in data {
            let mut expanded_byte = Self::expand_byte(*byte, self.inverted);
            expanded.extend_from_slice(&expanded_byte);
        }

        // 'RESET'
        for _ in 0..40 {
            expanded.push(reset_byte);
        }

        self.spi.transfer(&expanded, &mut []).await;
    }

    // NOTE: The data will be transfered MSB first.
    fn expand_byte(v: u8, inverted: bool) -> [u8; 8] {
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

            if inverted {
                buf[i] = !buf[i];
            }
        }

        buf
    }
}

