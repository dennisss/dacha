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

use core::convert::{AsRef, AsMut};

use common::fixed::vec::FixedVec;

use crate::spi::*;
use crate::pins::*;
use crate::gpio::*;
use crate::rtc::*;

/*
const SPI_FREQUENCY: usize = 8_000_000;

const RESET_LENGTH: usize = 80;

const EXPANDED_BYTE_LENGTH: usize = 16;
*/

const SPI_FREQUENCY: usize = 4_000_000;

/// 40 bytes takes 80us to transfer
const RESET_LENGTH: usize = 40;

const EXPANDED_BYTE_LENGTH: usize = 8;

pub struct Neopixel {
    spi: SPIHost,
    inverted: bool,
}

impl Neopixel {
    pub fn new(periph: SPIMx, mut pin: GPIOPin, dummy_clk_pin: GPIOPin, inverted: bool) -> Self {
        // Note that for inverted mode, there is no 'simple' way to keep this
        // pin high when the SPI peripheral is inactive so we rely on creating
        // a reset period during the SPI transfer for inverted mode in 'write'.
        pin
        .reset()
        .set_direction(PinDirection::Output)
        .write(PinLevel::Low);

        // TODO: Switch this to using sequenced PWM output to avoid needing a dummy SPI pin and
        // an extra large buffer for expanding bits. 
        let spi = SPIHost::new::<_, DisconnectedPin, _>(
            periph,
            SPI_FREQUENCY,
            Some(pin),
            None,
            Some(dummy_clk_pin),
            None,
            SPIMode::Mode0,
        );

        Self { spi, inverted }
    }

    pub fn into_inner(self) -> SPIMx {
        self.spi.into_inner()
    }

    pub async fn write<T: AsRef<[u8]>>(&mut self, data: &NeopixelDataBuffer<T>) {
        self.spi.transfer(data.data.as_ref(), &mut []).await;
    }


}

pub struct NeopixelDataBuffer<T> {
    data: T,
    inverted: bool
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> NeopixelDataBuffer<T> {
    
    pub const fn size_for(num_bytes: usize) -> usize {
        RESET_LENGTH + (num_bytes * EXPANDED_BYTE_LENGTH) + RESET_LENGTH
    }
    
    pub fn new(mut data: T, inverted: bool) -> Self {
        // TODO: Verify the buffer is an even number of some number of bits.
        assert!(data.as_ref().len() >= 2 * RESET_LENGTH);

        let reset_byte = if inverted { 0xff } else { 0x00 };

        // The data starts and ends with a 'reset' signal.
        let d = data.as_mut();
        let len = d.len();
        for i in 0..RESET_LENGTH {
            d[i] = reset_byte;
            d[len - i - 1] = reset_byte;
        }

        Self {
            data,
            inverted
        }
    }

    pub fn write(&mut self, index: usize, data: &[u8]) {
        // TODO: Bounds check the writes and 
        
        let mut buffer_i = RESET_LENGTH + index * EXPANDED_BYTE_LENGTH;

        for i in 0..data.len() {
            let out = array_mut_ref!(self.data.as_mut(), buffer_i, EXPANDED_BYTE_LENGTH);
            Self::expand_byte(data[i], out);

            if self.inverted {
                // TODO: This should be optimizable to a few u32 width operations.
                for b in out.iter_mut() {
                    *b = !*b;
                }
            }

            buffer_i += EXPANDED_BYTE_LENGTH;
        }
    }

    // NOTE: The data will be transfered MSB first.
    fn expand_byte(v: u8, buf: &mut [u8; EXPANDED_BYTE_LENGTH]) {
        for i in 0..8 {
            let bit = (v >> (7 - i)) & 1;

            if SPI_FREQUENCY == 8_000_000 {
                buf[2*i] = {
                    if bit != 0 {
                        0b11111100
                    } else {
                        0b11100000
                    }
                };
                buf[2*i + 1] = 0;
            } else {
                buf[i] = {
                    if bit != 0 {
                        0b11100000
                    } else {
                        0b10000000
                    }
                };
            }
        }
    }
}


