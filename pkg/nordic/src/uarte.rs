use core::arch::asm;
use core::ops::Drop;
use core::pin::Pin;

use peripherals::raw::register::{RegisterRead, RegisterWrite};
use peripherals::raw::uarte0::UARTE0;
use peripherals::raw::{Interrupt, InterruptState, PinDirection};

use crate::pins::PeripheralPin;

/// Internal implementation details:
/// - After new() is called, the peripheral is in the 'idle' state where:
///   - No transfers are in progress
///   - All interrupts ever are enabled (and never disabled).
///   - All events are cleared.
/// - When a read/write operation starts, it can assume that all events are
///   initially cleared but once those operations are done, they should clear
///   the events.
///   - If read/write futures are cancelled, they will block until in the 'idle'
///     state.
pub struct UARTE {
    reader: UARTEReader,
    writer: UARTEWriter,
}

impl UARTE {
    pub fn new<TXPin: PeripheralPin, RXPin: PeripheralPin>(
        mut periph: UARTE0,
        txd: TXPin,
        rxd: RXPin,
        baudrate: usize,
    ) -> Self {
        periph.enable.write_enabled();

        // TODO: Once the reader and writer is dropped (both pins disconnected),

        match baudrate {
            1200 => periph.baudrate.write_baud1200(),
            2400 => periph.baudrate.write_baud2400(),
            4800 => periph.baudrate.write_baud4800(),
            9600 => periph.baudrate.write_baud9600(),
            14400 => periph.baudrate.write_baud14400(),
            19200 => periph.baudrate.write_baud19200(),
            28800 => periph.baudrate.write_baud28800(),
            31250 => periph.baudrate.write_baud31250(),
            38400 => periph.baudrate.write_baud38400(),
            56000 => periph.baudrate.write_baud56000(),
            57600 => periph.baudrate.write_baud57600(),
            76800 => periph.baudrate.write_baud76800(),
            115200 => periph.baudrate.write_baud115200(),
            230400 => periph.baudrate.write_baud230400(),
            250000 => periph.baudrate.write_baud250000(),
            460800 => periph.baudrate.write_baud460800(),
            921600 => periph.baudrate.write_baud921600(),
            1000000 => periph.baudrate.write_baud1m(),
            _ => {} // TODO: Return an error.
        }

        periph.config.write_with(|v| v); // Defaults to 8N1
        periph.psel.txd.write_with(|v| {
            v.set_connect_with(|v| v.set_connected())
                .set_port(txd.port() as u32)
                .set_pin(txd.pin() as u32)
        });
        periph.psel.rxd.write_with(|v| {
            v.set_connect_with(|v| v.set_connected())
                .set_port(rxd.port() as u32)
                .set_pin(rxd.pin() as u32)
        });

        periph.inten.write_with(|v| {
            v.set_txstopped(InterruptState::Enabled)
                .set_endtx(InterruptState::Enabled)
                .set_rxto(InterruptState::Enabled)
                .set_endrx(InterruptState::Enabled)
        });

        // NOTE: We assume that events_endrx and others are not generated.

        Self {
            reader: UARTEReader {
                periph: unsafe { UARTE0::new() },
            },
            writer: UARTEWriter { periph: periph },
        }
    }

    pub fn split(self) -> (UARTEReader, UARTEWriter) {
        (self.reader, self.writer)
    }

    pub async fn write(&mut self, data: &[u8]) {
        self.writer.write(data).await
    }

    pub async fn read_exact(&mut self, data: &mut [u8]) {
        self.reader.read_exact(data).await
    }
}

pub struct UARTEWriter {
    periph: UARTE0,
}

impl UARTEWriter {
    // pub fn begin_write<'a>(&'a mut self, data: &'a mut [u8]) -> UARTEWrite<'a> {
    //     //
    // }

    pub async fn write(&mut self, data: &[u8]) {
        // NOTE: EasyDMA can only allow data in RAM.
        // TODO: Support larger flash data with segmented transfers.
        let mut buf = [0u8; 256];
        buf[0..data.len()].copy_from_slice(data);

        self.periph
            .txd
            .ptr
            .write(unsafe { core::mem::transmute(buf.as_ptr()) });
        self.periph.txd.maxcnt.write(data.len() as u32);

        self.periph.tasks_starttx.write_trigger();

        let mut write = UARTEWrite {
            writer: self,
            data: Pin::new(&buf),
            running: true,
        };

        write.wait().await;
    }
}

struct UARTEWrite<'a> {
    writer: &'a mut UARTEWriter,
    data: Pin<&'a [u8]>,
    running: bool,
}

// If a write is dropped, we must stop the DMA transfer to ensure that corrupt
// memory isn't sent out.
impl<'a> Drop for UARTEWrite<'a> {
    fn drop(&mut self) {
        self.cancel_blocking();
    }
}

impl<'a> UARTEWrite<'a> {
    pub async fn wait(&mut self) {
        while self.writer.periph.events_endtx.read().is_notgenerated() {
            executor::interrupts::wait_for_irq(Interrupt::UARTE0_UART0).await;
        }
        self.writer.periph.events_endtx.write_notgenerated();
        self.running = false;
        // assert_eq!(self.periph.txd.amount.read(), data.len() as u32);
    }

    async fn cancel(mut self) {
        if !self.running {
            return;
        }

        self.writer.periph.tasks_stoptx.write_trigger();

        // NOTE: We assume that TXSTOPPED is delivered after ENDTX.
        loop {
            if self.writer.periph.events_txstopped.read().is_generated() {
                break;
            }

            // Clearing any other events that would immediately re-trigger an interrupt.
            self.writer.periph.events_endtx.write_notgenerated();

            executor::interrupts::wait_for_irq(Interrupt::UARTE0_UART0).await;
        }

        self.writer.periph.events_txstopped.write_notgenerated();
        self.writer.periph.events_endtx.write_notgenerated();
        crate::events::flush_events_clear();

        self.running = false;
    }

    // TODO: Deduplicate in terms of the above one,
    fn cancel_blocking(&mut self) {
        if !self.running {
            return;
        }

        self.writer.periph.tasks_stoptx.write_trigger();

        // NOTE: We assume that TXSTOPPED is delivered after ENDTX.
        loop {
            if self.writer.periph.events_txstopped.read().is_generated() {
                break;
            }

            // Clearing any other events that would immediately re-trigger an interrupt.
            self.writer.periph.events_endtx.write_notgenerated();

            // Block
        }

        self.writer.periph.events_txstopped.write_notgenerated();
        self.writer.periph.events_endtx.write_notgenerated();
        crate::events::flush_events_clear();

        self.running = false;
    }
}

pub struct UARTEReader {
    periph: UARTE0,
}

impl UARTEReader {
    /// Starts performing a read
    pub fn begin_read<'a>(&'a mut self, data: &'a mut [u8]) -> UARTERead<'a> {
        let data = Pin::new(data);

        self.periph
            .rxd
            .ptr
            .write(unsafe { core::mem::transmute(data.as_ptr()) });
        // On NRF52 this can be up to 0xFFFF
        self.periph.rxd.maxcnt.write(data.len() as u32);

        self.periph.tasks_startrx.write_trigger();

        UARTERead {
            reader: self,
            data,
            running: true,
        }
    }

    pub async fn read_exact(&mut self, data: &mut [u8]) {
        let mut read = self.begin_read(data);
        read.wait().await;
    }

    // TODO: We can implement a read_until which uses a shortcut to immediately
    // start reading the next byte once one it done.

    // Ideally we could support timeout based reading.
    // ^ We can call STOPRX to force the RXTO event to be triggered
    // NOTE: ENDRX is always generated after STOPRX if applicable.
}

/// NOTE: There is no good way to track the
pub struct UARTERead<'a> {
    reader: &'a mut UARTEReader,
    data: Pin<&'a mut [u8]>,

    /// If true, we are waiting for either the main transfer or for flushing to
    /// be completed.
    running: bool,
}

// If a read is dropped, we must stop the DMA transfer to ensure that the memory
// can be freed.
impl<'a> Drop for UARTERead<'a> {
    fn drop(&mut self) {
        self.cancel_blocking();
    }
}

impl<'a> UARTERead<'a> {
    /// Waits for the
    pub async fn wait(&mut self) {
        // TODO: If we support parity bits, handle errors.

        // assert!(self.running);

        while self.reader.periph.events_endrx.read().is_notgenerated() {
            executor::interrupts::wait_for_irq(Interrupt::UARTE0_UART0).await;
        }
        self.reader.periph.events_endrx.write_notgenerated();
        self.reader.periph.events_rxdrdy.write_notgenerated();
        self.running = false;
        crate::events::flush_events_clear();
    }

    // TODO: Support using ENDRX_STARTRX shortcut to support reading with dual
    // buffering.

    // TODO: Also do clearing of EVENTS_RXDRDY

    /// Returns true if no data has been received yet. If false, then 1 or more
    /// bytes have been received. We can't reliably tell how many bytes have
    /// been received until the transfer is completed with wait() or cancel().
    pub fn is_empty(&self) -> bool {
        self.reader.periph.events_rxdrdy.read().is_notgenerated()
    }

    pub async fn cancel(mut self) -> usize {
        if !self.running {
            return 0;
        }

        self.reader.periph.tasks_stoprx.write_trigger();
        while self.reader.periph.events_rxto.read().is_notgenerated() {
            // Clearing any other events that would immediately re-trigger an interrupt.
            self.reader.periph.events_endrx.write_notgenerated();

            executor::interrupts::wait_for_irq(Interrupt::UARTE0_UART0).await;
        }

        self.reader.periph.events_endrx.write_notgenerated();
        self.reader.periph.events_rxto.write_notgenerated();
        self.reader.periph.events_rxdrdy.write_notgenerated();
        crate::events::flush_events_clear();

        let mut num_read = self.reader.periph.rxd.amount.read() as usize;

        // TODO: RXD.AMOUNT doesn't seem to change in value after a flush if the FIFO is
        // empty. We would need to compare the new and old values.
        /*
        let num_remaining = self.data.len() - num_read;
        if num_remaining > 0 {
            // Advance the pointer to the end of the read portion.
            self.reader
                .periph
                .rxd
                .ptr
                .write(self.reader.periph.rxd.ptr.read() + (num_read as u32));
            self.reader.periph.rxd.maxcnt.write(num_remaining as u32);

            self.reader.periph.tasks_flushrx.write_trigger();

            self.wait().await;

            num_read += self.reader.periph.rxd.amount.read() as usize;
        }
        */

        self.running = false;

        num_read
    }

    // TODO: Deduplicate in terms of the above.
    fn cancel_blocking(&mut self) {
        if !self.running {
            return;
        }

        self.reader.periph.tasks_stoprx.write_trigger();
        while self.reader.periph.events_rxto.read().is_notgenerated() {
            // Clearing any other events that would immediately re-trigger an interrupt.
            self.reader.periph.events_endrx.write_notgenerated();

            // Block
        }

        self.reader.periph.events_endrx.write_notgenerated();
        self.reader.periph.events_rxto.write_notgenerated();
        self.reader.periph.events_rxdrdy.write_notgenerated();
        crate::events::flush_events_clear();

        // No point in flushing as this code path is only used when the entire read is
        // dropped.

        self.running = false;
    }
}
