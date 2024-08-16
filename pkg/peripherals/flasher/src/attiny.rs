use std::time::Duration;

use common::errors::*;
use peripherals::gpio::*;
use peripherals::spi::*;

const NUM_SIGNATURE_BYTES: usize = 3;
const WORD_SIZE: usize = 2;

struct ChipInfo {
    name: &'static str,

    signature: [u8; NUM_SIGNATURE_BYTES],

    /// Number of words per flash page
    flash_page_words: usize,

    flash_num_pages: usize,
}

const CHIPS: &'static [ChipInfo] = &[
    ChipInfo {
        name: "ATtiny25",
        signature: [0x1E, 0x91, 0x08],
        flash_page_words: 16,
        flash_num_pages: 64,
    },
    ChipInfo {
        name: "ATtiny45",
        signature: [0x1E, 0x92, 0x06],
        flash_page_words: 32,
        flash_num_pages: 64,
    },
    ChipInfo {
        name: "ATtiny85",
        signature: [0x1E, 0x93, 0x0B],
        flash_page_words: 32,
        flash_num_pages: 64,
    },
];

pub struct ATTinyProgrammer {
    spi: SPIDevice,
    reset_pin: GPIOPin,

    /// When set, we are programming mode.
    chip_info: Option<&'static ChipInfo>,
}

impl ATTinyProgrammer {
    pub fn new(mut spi: SPIDevice, mut reset_pin: GPIOPin) -> Result<Self> {
        // reset_pin.set_mode(Mode::Output).write(true);
        reset_pin.configure(GPIOLineFlags::OUTPUT)?;
        reset_pin.write(true)?;

        // The default clock frequency is 1Mhz and the min low/high pulse width is 2
        // clock cycles (so max speed is around 250kHz by default).
        spi.set_speed_hz(100_000)?;

        spi.set_mode(0)?;

        Ok(Self {
            spi,
            reset_pin,
            chip_info: None,
        })
    }

    pub async fn enter_programming_mode(&mut self) -> Result<()> {
        if self.chip_info.is_some() {
            return Err(err_msg("Already in programming mode."));
        }

        // NOTE: We assume that the SPI driver holds SCK at low when not sending
        // anything.

        // Algorithm from section 20.5.1 of the ATTiny85 datasheet.
        // TODO: Retry this if we get no echo as recommended in the datasheet.
        {
            // Send a high, low, high, low RESET sequence at ensure the reset is detected.
            for i in 0..4 {
                self.reset_pin.write(i % 2 == 0)?;
                executor::sleep(Duration::from_millis(50)).await?;
            }

            {
                let mut out = [0u8; 2];
                self.spi.transfer(&[0xAC, 0x53], &mut out)?;

                if out[0] != 0x53 {
                    return Err(err_msg("Failed to enter programming mode"));
                }
            }
        }

        {
            let signature = self.read_signature_bytes()?;

            for chip in CHIPS {
                if chip.signature == signature {
                    println!("Connected to {}", chip.name);
                    self.chip_info = Some(chip);
                    break;
                }
            }
        }

        Ok(())
    }

    fn read_signature_bytes(&mut self) -> Result<[u8; NUM_SIGNATURE_BYTES]> {
        let mut out = [0u8; NUM_SIGNATURE_BYTES];

        for i in 0..NUM_SIGNATURE_BYTES {
            let mut send = [0x30, 0x00, i as u8];
            self.spi.transfer(&send, &mut out[i..(i + 1)])?;
        }

        Ok(out)
    }

    pub async fn exit_programming_mode(&mut self) -> Result<()> {
        if self.chip_info.is_none() {
            return Err(err_msg("Not in programming mode."));
        }

        self.reset_pin.write(true)?;
        executor::sleep(Duration::from_millis(50)).await?;
        self.chip_info = None;
        Ok(())
    }

    /// Gets the size of a flash page in bytes.
    pub fn flash_page_size_bytes(&self) -> Result<usize> {
        let chip_info = self
            .chip_info
            .ok_or_else(|| err_msg("Not in programming mode"))?;

        let page_size_bytes = chip_info.flash_page_words * WORD_SIZE;

        Ok(page_size_bytes)
    }

    pub async fn erase_chip(&mut self) -> Result<()> {
        self.spi.transfer(&[0xAC, 0x80, 0x00, 0x00], &mut [])?;
        executor::sleep(Duration::from_millis(100)).await?;
        Ok(())
    }

    /// NOTE: It is the callers responsibility to ensure that only full aligned
    /// pages are written.
    ///
    /// NOTE: erase_chip() must be called before re-programming the chip.
    pub async fn flash_write(&mut self, byte_offset: usize, data: &[u8]) -> Result<()> {
        let chip_info = self
            .chip_info
            .ok_or_else(|| err_msg("Not in programming mode"))?;

        let page_size_bytes = chip_info.flash_page_words * WORD_SIZE;
        if byte_offset % page_size_bytes != 0 {
            return Err(err_msg("Can only write at page offsets"));
        }

        if data.len() % page_size_bytes != 0 {
            return Err(err_msg("Can only write full pages"));
        }

        if byte_offset + data.len()
            > (chip_info.flash_num_pages * chip_info.flash_page_words * WORD_SIZE)
        {
            return Err(err_msg("Data overflows end of flash"));
        }

        if data.len() == 0 {
            return Ok(());
        }

        let page_mask = (chip_info.flash_page_words - 1) as u16;

        let mut i = 0;
        while i < data.len() {
            let addr = ((byte_offset + i) / WORD_SIZE) as u16;

            let addr_low = (addr & page_mask).to_be_bytes();
            // "Load Program Memory Page, Low byte"
            self.spi
                .transfer(&[0x40, addr_low[0], addr_low[1], data[i]], &mut [])?;
            // "Load Program Memory Page, High byte"
            self.spi
                .transfer(&[0x48, addr_low[0], addr_low[1], data[i + 1]], &mut [])?;

            i += WORD_SIZE;

            // Flush full pages to flash.
            if i % page_size_bytes == 0 {
                // "Write Program Memory Page"
                let addr_high = (addr & !page_mask).to_be_bytes();
                self.spi
                    .transfer(&[0x4C, addr_high[0], addr_high[1], 0x00], &mut [])?;

                while self.poll_busy()? {
                    executor::sleep(Duration::from_millis(1)).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn flash_read(&mut self, byte_offset: usize, out: &mut [u8]) -> Result<()> {
        // TODO: Need more bounds checks in this.

        if byte_offset % WORD_SIZE != 0 {
            return Err(err_msg("Can only read word aligned data"));
        }

        let mut i = 0;
        while i < out.len() {
            let addr = ((byte_offset + i) / WORD_SIZE) as u16;

            let addr_bin = addr.to_be_bytes();

            // "Read Program Memory, Low byte"
            self.spi
                .transfer(&[0x20, addr_bin[0], addr_bin[1]], &mut out[i..(i + 1)])?;
            // "Read Program Memory, High byte"
            self.spi.transfer(
                &[0x28, addr_bin[0], addr_bin[1]],
                &mut out[(i + 1)..(i + 2)],
            )?;

            i += WORD_SIZE;
        }

        Ok(())
    }

    fn poll_busy(&mut self) -> Result<bool> {
        let mut out = [0u8; 1];
        self.spi.transfer(&[0xF0, 0x00, 0x00], &mut out)?;
        Ok(out[0] & 1 == 1)
    }
}
