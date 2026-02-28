use common::fixed::vec::FixedVec;
use common::fixed::queue::FixedQueue;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode, SPITransferRequest
};
use peripherals::raw::Interrupt;
use cnc::time_remaining_u32;

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

    requests: FixedQueue<SPISamplingRequest, 2>,

    /// Index of the next request in 'requests' which is still active.
    next_request: usize,
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

    request_status: PeripheralResponse_ErrorCode,
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
            requests: FixedQueue::default(),
            next_request: 0,
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
        if self.requests.len() == self.requests.capacity() {
            return false;
        }

        buffer.view_mut::<u8>().set_used(0);

        self.requests.push_back(SPISamplingRequest {
            next_time: request.start_time(),
            num_samples_plus_1: request.transfer_count() + 2,
            interval: request.transfer_interval(),
            write_buffer: request.data().into(),
            read_buffer: buffer,
            read_buffer_index: request.read_buffer() as usize,
            request_sequence,
            request_status: PeripheralResponse_ErrorCode::NO_ERROR
        });

        true
    }

    pub fn read_completed_request(&mut self) -> Option<(PeripheralResponse_ErrorCode, u8, Buffer, usize)> {
        if self.next_request == 0 {
            return None;
        }

        let req = self.requests.pop_front().unwrap();
        self.next_request -= 1;

        Some((req.request_status, req.request_sequence, req.read_buffer, req.read_buffer_index))
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

        while self.next_request != self.requests.len() {
            let mut request = &mut self.requests[self.next_request];

            // Just finished. Issue a notification 
            if request.num_samples_plus_1 == 1 {
                self.next_request += 1;
                executor::interrupts::trigger_irq(Interrupt::EGU0_SWI0);

                // Start the next request if there is any.
                continue;
            }

            // Wait for the last SPI transfer to finish.
            //
            // NOTE: This currently imples that if there must be a time gap between
            // consecutive SPI requests since we don't support simultaneously starting
            // the next one as the current one finishes. 
            if request.num_samples_plus_1 == 2 {
                if !Self::try_set_compare_value(request.next_time, &mut self.timer_channel) {
                    request.request_status = PeripheralResponse_ErrorCode::TIMEOUT;
                    self.next_request += 1;
                    executor::interrupts::trigger_irq(Interrupt::EGU0_SWI0);
                    continue;
                }

                request.num_samples_plus_1 = 1;

                self.timer_channel.enable_interrupt();
                self.waiting_for_timer = true;
                return;
            }

            // Setup transfer
            // TODO: Need to bounda check the buffer length.
            {
                let mut read_buf = request.read_buffer.view_mut::<u8>();

                let i = read_buf.used();
                let j = i + request.write_buffer.len();
                read_buf.set_used(j);

                let buf = &mut read_buf.raw()[i..j];

                self.spi.setup_transfer(&request.write_buffer, buf);
            }


            if !Self::try_set_compare_value(request.next_time, &mut self.timer_channel) {
                request.request_status = PeripheralResponse_ErrorCode::TIMEOUT;
                self.next_request += 1;
                executor::interrupts::trigger_irq(Interrupt::EGU0_SWI0);
                continue;
            }

            request.next_time += request.interval;
            request.num_samples_plus_1 -= 1;

            self.timer_channel.enable_interrupt();
            self.ppi_channel.enable();
            self.waiting_for_timer = true;

            return;
        }
    }

    fn try_set_compare_value(next_time: u32, timer_channel: &mut TimerChannel<'static>) -> bool {
        const MIN_REMAINING_TIME: u32 = 50;
        const MAX_REMAINING_TIME: u32 = 10 * 16_000_000; // 4 seconds

        let current_time = timer_channel.capture();

        let delta_time = time_remaining_u32(next_time, current_time);

        let too_slow = delta_time < MIN_REMAINING_TIME || delta_time >= MAX_REMAINING_TIME;
        if unsafe { core::intrinsics::unlikely(too_slow) } {
            return false;
        }

        timer_channel.set_compare_value(next_time);

        // If the timer channel hasn't been used in a while, it may have a stale event pending so we
        // need to clear that.
        //
        // It's also undefined if the above current_time capture will trigger a new event
        // immediately.
        //
        // TIMING: This must run at least one clock cycle after the capture() to ensure we clear any
        // event caused by that.
        let _ = timer_channel.clear_pending_no_wait();

        true
    }
}