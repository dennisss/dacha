#[macro_use]
extern crate macros;

use std::time::Duration;

use common::errors::*;
use rpi::pcm::*;
use rpi::{clock::*, gpio::GPIO, pwm::SysPWM};

/// Target of 0.4us per cycle.
const TARGET_RATE: usize = 2500000;
const TARGET_PERIOD: Duration = Duration::from_nanos(400);

/// When not writing colors, you should sleep this long to ensure the LEDs reset
/// to the next data cycle.
pub const WS2812_RESET_TIME: Duration = Duration::from_micros(100);

// When not writing serial data, the serial bit should be held low.
// NOTE: Because colors end in a 0

/// Serializes colors to be written via a serial logic pin to a WS2812
/// compatible LED chain.
///
/// The WS2812 protocol as follows:
/// - During each refresh cycle, LED colors are written sequentially in the
///   order they are chained together in.
/// - Each color is 24bits (3 bytes each in MSB order : GRB color order).
/// - Each bit is formed by the DIN pin on the LED doing the following:
///     - For a '0' bit
///         - Go high for 0.2 to 0.4us
///         - Go low for >0.8us
///     - For a '1' bit
///         - Go high for 0.58 to 1.0us
///         - Stay low for >0.2us
/// - Cycles are delimited by periods of >80us of the DIN pins staying low.
///
/// We emulate this by assuming we have a serial output pin which writes 1 bit
/// (0 or 1 logic level) every 0.4us. Then 1 LED bit corresponds to 3 serial
/// bits which are either '110' for high or '100' for low.
///
/// Note that when the serial pin is in-active, it should be held low. Note that
/// if the implementation retains the last bit in the serial bit stream, then
/// the color bit stream is always guaranteed to end in a low logic level.
#[derive(Default)]
struct WS2812ColorSerializer {
    current_word: u32,

    /// Position of the next bit in 'current_word' to be written.
    /// This ranges from [0, 31]
    current_bit: usize,

    words: Vec<u32>,
}

impl WS2812ColorSerializer {
    /// Creates a new serializer containing no colors.
    pub fn new() -> Self {
        Self {
            current_word: 0,
            current_bit: 31,
            words: vec![],
        }
    }

    /// Pushes a color to the end of the chain.
    ///
    /// 'rgb' should have its 24 lowest bits set to 3 groups of 8-bits.
    ///
    /// The order of the groups from MSB to LSB should be 0{8} R{8} G{8} B{8}
    pub fn add_color(&mut self, rgb: u32) {
        // Switch from RGB to GRB
        let r_bits = (16..24).rev();
        let g_bits = (8..16).rev();
        let b_bits = (0..8).rev();
        let grb_bits = g_bits.chain(r_bits).chain(b_bits);

        for i in grb_bits {
            let bit = (rgb >> i) & 1;

            self.add_bit(1);
            self.add_bit(bit);
            self.add_bit(0);
        }
    }

    fn add_bit(&mut self, v: u32) {
        debug_assert!(v & 1 == v); // Just one bit given

        self.current_word |= v << self.current_bit;

        if self.current_bit == 0 {
            self.words.push(self.current_word);
            self.current_word = 0;
            self.current_bit = 31;
        } else {
            self.current_bit -= 1;
        }
    }

    /// Returns the complete sequence of serial words representing the colors.
    pub fn finish(mut self) -> Vec<u32> {
        // Push the last incomplete word padded with zeros.
        if self.current_bit != 31 {
            self.words.push(self.current_word);
        }

        self.words
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let mut gpio = GPIO::open()?;

    let mut pin = gpio.pin(21);

    let mut cm = ClockManager::open()?;
    let osc_rate = cm.oscillator_rate().await?;

    println!("Oscillator Freq: {}", osc_rate);

    let divi = osc_rate / TARGET_RATE;
    println!("DIVI: {}", divi);

    cm.configure(Clock::PCM, ClockSource::Oscillator, divi as u16);

    // pin.set_mode(rpi::gpio::Mode::Output);
    pin.set_resistor(rpi::gpio::Resistor::PullDown);
    // pin.write(true);

    // TODO: Re-calculate the period based on the divided oscillator rate.
    let mut pcm = PCM::open(pin, TARGET_PERIOD)?;

    // std::thread::sleep(Duration::from_secs(4));

    let colors = &[0, 0xff, 0xff00, 0xff0000];

    loop {
        for c in colors {
            println!("Write!");

            let mut serializer = WS2812ColorSerializer::new();
            serializer.add_color(*c);

            let data = serializer.finish();

            pcm.write(&data[..]);
            std::thread::sleep(WS2812_RESET_TIME);

            std::thread::sleep(Duration::from_secs(1));
        }
    }

    Ok(())
}