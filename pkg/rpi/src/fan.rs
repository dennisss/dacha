use std::time::{Duration, Instant};

use common::errors::*;

use crate::gpio::*;

const MAX_WAIT_TIME: Duration = Duration::from_millis(60);
const POLLING_PERIOD: Duration = Duration::from_micros(100);
const SAMPLING_ROUNDS: usize = 10;

/// Standard PWM frequency for controlling the speed of PWM fans.
pub const FAN_PWM_FREQUENCY: f32 = 25000.0;

/// Reads the speed of a standard 4-pin PWM computer fan using the tachometer
/// output pin directly connected to the Pi.
///
/// A standard fan will drive the tachometer pin to low twice per revolution.
/// This will continously poll the level of the pin to look from a 1 -> 0 -> 1
/// -> 0 transition where the measured period is the time between consecutive
/// falling edges. Currently this is implemented without interrupts.
///
/// The fastest fan we will support is an NF-A4x20:
/// - RPM range is 1200 RPM - 5000 RPM
/// - So will get a pulse at 40 Hz - 166 Hz
/// - So period between falling edges is `[5ms, 25ms]`
pub struct FanTachometerReader {
    pin: GPIOPin,
}

impl FanTachometerReader {
    pub fn create(mut pin: GPIOPin) -> Self {
        pin.set_mode(Mode::Input).set_resistor(Resistor::PullUp);
        Self { pin }
    }

    /// Returns the estimated RPM of the fan.
    pub async fn read(&mut self) -> Result<usize> {
        // Individual samples are fairly noisy so sample several times to get a better
        // measurement.

        let mut rpm = 0;
        for _ in 0..SAMPLING_ROUNDS {
            rpm += self.read_once().await?;
        }

        rpm /= SAMPLING_ROUNDS;
        Ok(rpm)
    }

    async fn read_once(&mut self) -> Result<usize> {
        let start_time = Instant::now();
        let end_time = start_time + MAX_WAIT_TIME;

        // Initialized later.
        let mut t1 = start_time;
        let mut t2 = start_time;

        let mut state = 0;
        loop {
            let t = Instant::now();
            if t >= end_time {
                return Ok(0);
            }

            let v = self.pin.read();

            match state {
                // Wait for a 1
                0 => {
                    if v {
                        state = 1;
                    }
                }
                // Wait for a 0
                1 => {
                    if !v {
                        t1 = t;
                        state = 2;
                    }
                }
                // Wait for a 1
                2 => {
                    if v {
                        state = 3
                    }
                }
                // Wait for a 0
                3 => {
                    if !v {
                        t2 = t;
                        break;
                    }
                }
                _ => todo!(),
            }

            std::thread::sleep(POLLING_PERIOD);
        }

        let rps = 0.5 / (t2 - t1).as_secs_f32();
        let rpm = rps * 60.0;

        Ok(rpm as usize)
    }
}
