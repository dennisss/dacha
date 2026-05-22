
/*
AR0234 Specifics:

- pixel_clock is 90_000_000 Hz 
    - This is also readable via the 'pixel_rate' v4l2 control

- line_length pck is located at sensor I2C address of 0x300C
    - The default / minimum value is '612'
    - The units of this number are pixel clock cycles.
    - So 'line_length = (612 / 90_000_000) seconds'

- The exposure control in the v4l2 driver is the same as the value of the exposure time register on the sensor.
    - The register defines the time as a multiple of the 'line_length_pck'
    - So 'exposure_time = exposure_value * (612 / 90_000_000) seconds'

- In the 'sync-sink' mode (pipelined exposure and sensor readout)
    - A high pulse to the external trigger aligns the NEXT frame's exposure to start aligned with the pulse
    - The sensor has 1200 lines so (612 / 90000000) * 1200 = 8160us
    - So the start of the start is 8160us after the last external trigger pulse.

*/

use std::time::Duration;

use ptp::SignedDuration;

#[derive(Default)]
pub struct AR0234Driver {

}

impl AR0234Driver {
    // The exact frame width doesn't matter for this sensor but
    // it must be long enough to be generatable by the pps_divider MCU and short enough
    // that it doesn't bleed into the next frame. 
    pub fn frame_width(&self) -> Duration {
        Duration::from_millis(1)
    }

    pub fn frame_offset(&self) -> SignedDuration {
        SignedDuration::from_micros(-8160)
    }

    pub fn exposure_time_to_value(&self, exposure_micros: u32) -> u32 {
        let mut v = exposure_micros as u64;
        v *= (90_000_000 / 1_000_000); // The '/ 1 million' is to convert from microseconds to seconds  
        v /= 612;
        v as u32
    }
}

