use common::errors::*;
use rpi::gpio::*;

use crate::spi::{SPIDevice, SPI};

/// Maximum buffer size allowed by the linux spidev driver before throwing an
/// error. https://github.com/torvalds/linux/blob/master/drivers/spi/spidev.c#L85
const MAX_SPI_BUFFER_SIZE: usize = 4096;

/// Driver for the sparkfun 1.8" TFT
///
/// Product Page: https://www.sparkfun.com/products/15143
///
/// Hookup:
/// - Minimal (4WSPI):
///   - SCLK
///   - MOSI
///   - LCDCS
///   - D/C
/// - Extra:
///   - PWM (backlight pin)
///   - TE (for vsync)
///
/// SPI Bus Parameters:
/// - Mode 0 (MSB First, CS Active low)
/// - 8-bit
/// - Max Frequency: ~15MHz
///
/// Specs:
/// - Module No: KWH018ST14-F01
/// - Driver: ILI9163C
/// - Resolution: 128x160
/// - 18-bit color (3 bytes per pixel with lower 2 bits per channel being
///   unused).
pub struct SparkFun18TFT {
    spi: SPIDevice,

    /// D/CX pin. For the purposes of 4WSPI, this has the following meaning:
    /// - Low: We are sending a command
    /// - High: We are sending data/parameters.
    dc: GPIOPin,

    backlight: GPIOPin,
}

impl SparkFun18TFT {
    pub fn open(spi_path: &str, dc_pin: usize, backlight_pin: usize) -> Result<Self> {
        let mut spi = SPIDevice::open(spi_path)?;
        spi.set_speed_hz(32_000_000)?;

        let gpio = GPIO::open()?;

        let mut backlight = gpio.pin(backlight_pin);
        backlight.set_mode(Mode::Output).write(true);

        let mut dc = gpio.pin(dc_pin);
        dc.set_mode(Mode::Output)
            .set_resistor(Resistor::None)
            .write(false);

        let mut inst = Self { spi, dc, backlight };
        inst.init()?;

        Ok(inst)
    }

    fn init(&mut self) -> Result<()> {
        /*
        Calibration parameters are from sparkfun's library:
        https://github.com/sparkfun/HyperDisplay_KWH018ST01_4WSPI_ArduinoLibrary/blob/6087a3ddaf7f74cfd970503c2d896788fde1f052/src/HyperDisplay_KWH018ST01_4WSPI.cpp
        */

        // 'Software Reset'
        self.write_command(0x01, &[])?;

        std::thread::sleep(std::time::Duration::from_millis(100));

        // 'SLPOUT' (Exit Sleep mode)
        self.write_command(0x11, &[])?;

        std::thread::sleep(std::time::Duration::from_millis(100));

        // 'Normal Display Mode On'
        self.write_command(0x13, &[])?;

        // 'Display Inversion Off'
        self.write_command(0x20, &[])?;

        // 'Gamma Set'
        self.write_command(0x26, &[0x04])?;

        // 'Column Address Set'
        self.write_command(0x2A, &[0, 0, 0, 159])?;

        // 'Page Address Set'
        self.write_command(0x2B, &[0, 0, 0, 127])?;

        // 'Memory Access Control'
        // - BGR order (seems to actually behave like RGB though)
        // - First pixel is in the top-left with the screen oriented long side
        //   horizontally.
        self.write_command(0x36, &[1 << 5 | 1 << 3 | 1 << 6])?;

        // 'Idle mode off'
        self.write_command(0x38, &[])?;

        // 'Interface Pixel Format'
        // 18-bit color
        self.write_command(0x3A, &[0x66])?;

        // 'Frame Rate Control (In normal mode / Full colors)'
        // These are the default values for this resolution: Frame rate = 61.7Hz
        self.write_command(0xb1, &[14, 20])?;

        // 'Power_Control1'
        self.write_command(0xc0, &[0x0c, 0x05])?;

        // 'Power_Control2'
        self.write_command(0xc1, &[0x02])?;

        // 'Power_Control3'
        self.write_command(0xc2, &[0x02])?;

        // 'VCOM_Control 1'
        self.write_command(0xc5, &[0x20, 0x55])?;

        // 'VCOM Offset Control'
        self.write_command(0xc7, &[0x40])?;

        // 'Source Driver Direction Control'
        self.write_command(0xB7, &[0])?;

        // 'Positive Gamma Correction'
        self.write_command(
            0xE0,
            &[
                0x36, 0x29, 0x12, 0x22, 0x1C, 0x15, 0x42, 0xB7, 0x2F, 0x13, 0x12, 0x0A, 0x11, 0x0B,
                0x06,
            ],
        )?;

        // 'Negative Gamma Correction'
        self.write_command(
            0xE1,
            &[
                0x09, 0x16, 0x2D, 0x0D, 0x13, 0x15, 0x40, 0x48, 0x53, 0x0C, 0x1D, 0x25, 0x2E, 0x34,
                0x39,
            ],
        )?;

        // 'GAM_R_SEL'
        self.write_command(0xF2, &[1])?;

        // Display On
        self.write_command(0x29, &[])?;

        Ok(())
    }

    fn write_command(&mut self, command: u8, mut params: &[u8]) -> Result<()> {
        self.dc.write(false);
        self.spi.transfer(&[command], &mut [])?;

        while params.len() > 0 {
            self.dc.write(true);

            let n = std::cmp::min(params.len(), MAX_SPI_BUFFER_SIZE);
            self.spi.transfer(&params[0..n], &mut [])?;

            params = &params[n..];
        }

        Ok(())
    }

    pub fn rows(&self) -> usize {
        128
    }

    pub fn cols(&self) -> usize {
        160
    }

    pub fn bytes_per_pixel(&self) -> usize {
        3
    }

    pub fn draw_frame(&mut self, frame: &[u8]) -> Result<()> {
        // Memory Write
        self.write_command(0x2c, frame)?;

        // NOP (to end to memory write)
        self.write_command(0, &[])?;

        Ok(())
    }
}
