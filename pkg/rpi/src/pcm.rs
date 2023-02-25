use std::sync::Arc;
use std::time::Duration;

use common::errors::*;

use crate::gpio::*;
use crate::memory::{MemoryBlock, PCM_PERIPHERAL_OFFSET};

struct PinSpec {
    number: usize,
    mode: Mode,
}

const DOUT_PINS: &[PinSpec] = &[
    PinSpec {
        number: 21,
        mode: Mode::AltFn0,
    },
    PinSpec {
        number: 31,
        mode: Mode::AltFn2,
    },
];

pub struct PCM {
    mem: Arc<MemoryBlock>,
    clock_period: Duration,
}

impl PCM {
    // Register offsets.
    const CS_A: usize = 0x00;
    const FIFO_A: usize = 0x04;
    const MODE_A: usize = 0x08;
    const RXC_A: usize = 0x0c;
    const TXC_A: usize = 0x10;
    const DREQ_A: usize = 0x14;
    const INTEN_A: usize = 0x18;
    const INTSTC_A: usize = 0x1c;
    const GRAY: usize = 0x20;

    /// Initializes the PCM peripheral with a DOUT pin configured.
    ///
    /// This will be configured to write out frames which are 32-bits in length
    /// and consist of just 1 channel. So basically 1 bit will be emitted per
    /// clock cycle with no padding.
    ///
    /// NOTE: This assumes that the PCM clock has already been configured and is
    /// running. The limit for the PCM clock is 25Mhz
    pub fn open(mut dout_pin: GPIOPin, clock_period: Duration) -> Result<Self> {
        let mut mem = MemoryBlock::open_peripheral(PCM_PERIPHERAL_OFFSET, 0x24)?;

        // Stop the PCM peripheral if already running.
        mem.write_register(Self::CS_A, 0);
        std::thread::sleep(Duration::from_micros(100));

        mem.write_register(
            Self::CS_A,
            (1 << 0), // EN
        );

        // Disable interrupts
        mem.write_register(Self::INTEN_A, 0);

        // Clear any previously asserted interrupts.
        mem.write_register(Self::INTSTC_A, 0b1111);

        mem.write_register(
            Self::MODE_A,
            (31 << 10) | // FLEN = 32
            (0 << 0), // FSLEN = 0
        );

        // Enable channel 1 with width of 32 bits.
        mem.write_register(
            Self::TXC_A,
            (1 << 31) | // CH1WEX
            (1 << 30) | // CH1EN
            (8  << 16) | // CH1WID
            (0 << 20), // CH1POS = 0
        );

        let spec = DOUT_PINS
            .iter()
            .find(|p| p.number == dout_pin.number())
            .ok_or_else(|| format_err!("Pin {} can't be used as PCM DOUT", dout_pin.number()))?;
        dout_pin.set_mode(spec.mode);

        // TXTHR = 0b00 (set TXW when FIFO is empty)
        mem.write_register(
            Self::CS_A,
            (1 << 0) | // EN
            (1 << 3) | // TXCLR
            (1 << 4) | // RXCLR
            (1 << 24), // SYNC
        );

        // Wait for the sync bit to be set which means 2 clock cycles have passed (so
        // any old state has been flushed).
        loop {
            let cs_a = mem.read_register(Self::CS_A);
            let sync = (cs_a >> 24) & 1;
            if sync == 1 {
                break;
            }

            std::thread::sleep(Duration::from_micros(10));
        }

        let mut inst = Self {
            mem: Arc::new(mem),
            clock_period,
        };

        Ok(inst)
    }

    /// Writes data to the PCM FIFO buffer which will be emitted from the DOUT
    /// pin MSB first.
    ///
    /// Only up to 64 words may be written as the current implementation
    /// requires data to fit in the FIFO (doesn't use DMA).
    ///
    /// Blocks until all data has been flushed.
    pub fn write(&mut self, data: &[u32]) {
        assert!(data.len() <= 64);

        // Initially FIFO is empty
        assert!(self.mem.read_register(Self::CS_A) & (1 << 17) != 0);

        for v in data {
            self.mem.write_register(Self::FIFO_A, *v);
        }

        // FIFO now above threshold (as no longer empty)
        assert!(self.mem.read_register(Self::CS_A) & (1 << 17) == 0);

        // Start transmitting.
        self.mem.write_register(
            Self::CS_A,
            self.mem.read_register(Self::CS_A) | (1 << 2), // TXON
        );

        // Estimated duration after which the data will be done being written.
        let eta = self.clock_period * (32 * (data.len() as u32) + 2);

        std::thread::sleep(eta);

        // Wait for TXW to be set indicating the FIFO is empty again.
        //
        // Note: Once all the FIFO data is consumed, we will also get a TXERR because of
        // the under-flow.
        loop {
            let txw = self.mem.read_register(Self::CS_A) & (1 << 17) != 0;
            if txw {
                break;
            }

            // If we are here, then we underestimated the time.
            std::thread::sleep(Duration::from_micros(2));
        }

        // Explicitly stop the transmission
        self.mem.write_register(
            Self::CS_A,
            self.mem.read_register(Self::CS_A) & !(1 << 2), // TXON
        );

        // self.check_error();
    }
}
