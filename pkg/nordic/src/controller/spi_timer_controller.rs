use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode, SPITransferRequest
};
use peripherals::raw::Interrupt;

use crate::spi::*;
use crate::controller::buffer::*;
use crate::ppi::*;
use crate::timer::*;


pub struct SPITimerController {
    spi: SPIHost,

    timer_channel: TimerChannel<'static>,

    ppi_channel: PPIChannel,

    /// If true, we are waiting for a timer interrupt before we can proceed with the current request.
    waiting_for_timer: bool,

    request: Option<SPISamplingRequest>
}

pub struct SPISamplingRequest {
    /// Start time of the next sample transfer.
    next_time: u32,

    /// Total number of transfers to perform (first one starting at next_time).
    num_samples_plus_1: u32,

    /// Number of timer ticks between transfer starts.
    interval: u32,

    write_buffer: FixedVec<u8, 8>,

    /// Where to write the results fo the transfers.
    read_buffer: Buffer,

    read_buffer_index: usize,

    request_sequence: u8,
}

impl SPITimerController {

    pub fn new(
        mut spi: SPIHost,
        ppi: &mut PPIChannels,
        timer: &'static Timer,
    ) -> Option<Self> {
        let timer_channel = match timer.new_channel() {
            Some(v) => v,
            None => return None
        };

        let ppi_channel = match ppi.new_channel(
            timer_channel.compare_event(),
            spi.start_task()
        ) {
            Some(v) => v,
            None => return None
        };

        Some(Self {
            spi,
            timer_channel,
            ppi_channel,
            waiting_for_timer: false,
            request: None
        })
    }

    pub fn into_inner(self) -> SPIHost {
        self.spi
    }

    pub fn enqueue_request(
        &mut self,
        request_sequence: u8,
        request: &SPITransferRequest,
        mut buffer: Buffer,
    ) -> bool {
        if self.request.is_some() {
            return false;
        }

        buffer.view_mut::<u8>().set_used(0);

        self.request = Some(SPISamplingRequest {
            next_time: request.start_time(),
            num_samples_plus_1: request.transfer_count() + 2,
            interval: request.transfer_interval(),
            write_buffer: request.data().into(),
            read_buffer: buffer,
            read_buffer_index: request.read_buffer() as usize,
            request_sequence
        });

        true
    }

    pub fn read_completed_request(&mut self) -> Option<(u8, Buffer, usize)> {
        match &self.request {
            Some(req) => {
                if req.num_samples_plus_1 != 0 {
                    return None;
                }
            },
            None => return None
        }

        let req = self.request.take().unwrap();

        Some((req.request_sequence, req.read_buffer, req.read_buffer_index))
    }

    // TODO: Debug how often this gets called to make sure we aren't overtriggering interrupts.

    /// Call this whenever there is a timer interrupt.
    pub fn tick(&mut self) {

        if self.waiting_for_timer {
            if self.timer_channel.pending_event_no_wait() {
                self.waiting_for_timer = false;

                self.timer_channel.disable_interrupt();

                // Should be last to ensure there is some time for the PPI channel to trigger
                self.ppi_channel.disable();

            } else {
                return;
            }
        }

        // TODO: Sanity check all the compare times aren't too close or in the past.

        let mut request = match &mut self.request {
            Some(v) => v,
            None => return
        };

        // Done
        if request.num_samples_plus_1 == 0 {
            return;
        }

        // Just finished. Issue a notification 
        if request.num_samples_plus_1 == 1 {
            request.num_samples_plus_1 = 0;
            executor::interrupts::trigger_irq(Interrupt::EGU0_SWI0);
            return;
        }

        // Wait for the last SPI transfer to finish.
        if request.num_samples_plus_1 == 2 {
            self.timer_channel.set_compare_value(request.next_time);
            request.num_samples_plus_1 = 1;

            self.timer_channel.enable_interrupt();
            self.waiting_for_timer = true;
            return;
        }

        // Setup transfer
        {
            let mut read_buf = request.read_buffer.view_mut::<u8>();

            let i = read_buf.used();
            let j = i + request.write_buffer.len();
            read_buf.set_used(j);

            let buf = &mut read_buf.raw()[i..j];

            self.spi.setup_transfer(&request.write_buffer, buf);
        }


        self.timer_channel.set_compare_value(request.next_time);
        request.next_time += request.interval;
        request.num_samples_plus_1 -= 1;

        self.timer_channel.enable_interrupt();
        self.ppi_channel.enable();
        self.waiting_for_timer = true;
    }
}