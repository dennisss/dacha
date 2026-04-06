use core::arch::asm;

use common::register::{RegisterRead, RegisterWrite};
use peripherals::raw::rtc0::RTC0;
use peripherals::raw::Interrupt;

/// If we are waiting for some target time to be reached, this is the minimum
/// number of RTC clock ticks between now and that time for us to use a COMPARE
/// interrupt. If fewer than this many ticks are left in the wait time we will
/// assume that we have already reached the target time.
///
/// This minimum is required as we don't want to accidentally run over the
/// target time while we are setting up the COMPARE registers. If we do run
/// over, we'd need to wait for a clock overflow.
///
/// Assuming we are running at 32KHz, this value is ~0.25ms.
const MIN_WAIT_TICKS: u32 = 8;

static mut NUM_WAITERS: u32 = 0;

pub struct RTC {
    rtc0: RTC0,
}

impl Clone for RTC {
    fn clone(&self) -> Self {
        Self {
            // Timer::new() gurantees that the RTC will only be used by instances of 'Timer'.
            // The internal implementation of wait_ticks is safe to use assuming the process is only
            // single threaded and won't be interrupted in the middle of it.
            rtc0: unsafe { RTC0::new() },
        }
    }
}

impl RTC {
    const TIMER_LIMIT: u32 = 1 << 24;

    /// NOTE: This function assumes that RTC0 is currently stopped.
    pub fn new(mut rtc0: RTC0) -> Self {
        // Errata 20
        rtc0.tasks_stop.write(peripherals::raw::TaskTrigger::Unknown(0));
        for _ in 0..10 {
            unsafe { asm!("nop") };
        }

        rtc0.prescaler.write(0); // Explictly request a 32.7kHz tick. (so max wait is 512 seconds)
        rtc0.tasks_start.write_trigger();

        // Wait for the first tick to know the RTC has started.
        let initial_count = rtc0.counter.read();
        while initial_count == rtc0.counter.read() {
            unsafe { asm!("nop") };
        }

        rtc0.cc[0].write_with(|v| v.set_compare(0));

        rtc0.events_compare[0].write_notgenerated();

        Self { rtc0 }
    }

    /// NOTE: This is only valid for 512 seconds after which point it will
    /// overflow.
    pub fn now(&self) -> RTCInstant {
        RTCInstant {
            ticks: self.rtc0.counter.read(),
        }
    }

    pub async fn wait_ms(&mut self, millis: u32) {
        self.wait_ticks((millis * 32768) / 1000).await
    }

    pub async fn wait_micros(&mut self, micros: u32) {
        self.wait_ticks((micros * 32768) / 1000000).await
    }

    async fn wait_ticks(&mut self, mut ticks: u32) {
        // TODO: Wrap in a critical section.
        
        // Enable interrupt on COMPARE0.
        // NOTE: We don't need to set EVTEN
        unsafe { 
            if NUM_WAITERS == 0 {
                self.rtc0.intenset.write_with(|v| v.set_compare0());
            }

            NUM_WAITERS += 1;
        }


        if ticks < MIN_WAIT_TICKS {
            ticks = MIN_WAIT_TICKS;
        }

        // TODO: This is not suitable for high speed operations as it takes ~375.2ns to
        // readthe current counter.
        let start_ticks = self.rtc0.counter.read();
        let end_ticks = (start_ticks + ticks) % Self::TIMER_LIMIT;

        loop {
            let current_ticks = self.rtc0.counter.read();
            let elapsed_ticks = Self::duration(start_ticks, current_ticks);
            if elapsed_ticks >= ticks {
                break;
            }

            let old_compare = self.rtc0.cc[0].read().compare();
            let old_target_duration = Self::duration(start_ticks, old_compare);

            // Override the compare register if our compare value is earlier that the
            // current one or the current one has already elapsed.
            if old_target_duration >= ticks || old_target_duration <= elapsed_ticks {
                self.rtc0.cc[0].write_with(|v| v.set_compare(end_ticks));
            }

            // NOTE: We don't initially clear EVENTS_COMPARE0 as another waiter may still
            // need to be woken up.

            executor::interrupts::wait_for_irq(Interrupt::RTC0).await;

            // Clear event so that the interrupt doesn't happen again.
            self.rtc0.events_compare[0].write_notgenerated();
        }

        // NOTE: We disable it mainly because pending events seem to consume power while the
        // interrupt is enabled.
        unsafe {
            NUM_WAITERS -= 1;
            
            if NUM_WAITERS == 0 {
                self.rtc0.intenclr.write_with(|v| v.set_compare0());
            }
        }
    }

    // TODO: Check for bugs.
    /*
    pub async fn wait_until(&mut self, time: RTCInstant) {
        let start_ticks = self.rtc0.counter.read();
        let ticks = if time.ticks > start_ticks {
            time.ticks - start_ticks
        } else {
            5
        };

        let end_ticks = time.ticks % Self::TIMER_LIMIT;

        loop {
            let current_ticks = self.rtc0.counter.read();
            let elapsed_ticks = Self::duration(start_ticks, current_ticks);
            if elapsed_ticks + MIN_WAIT_TICKS > ticks {
                break;
            }

            let old_compare = self.rtc0.cc[0].read().compare();
            let old_target_duration = Self::duration(start_ticks, old_compare);

            // Override the compare register if our compare value is earlier that the
            // current one or the current one has already elapsed.
            if old_target_duration >= ticks || old_target_duration <= elapsed_ticks {
                self.rtc0.cc[0].write_with(|v| v.set_compare(end_ticks));
            }

            // NOTE: We don't initially clear EVENTS_COMPARE0 as another waiter may still
            // need to be woken up.

            executor::interrupts::wait_for_irq(Interrupt::RTC0).await;

            // Clear event so that the interrupt doesn't happen again.
            self.rtc0.events_compare[0].write_notgenerated();
        }
    }
    */

    fn duration(start_ticks: u32, mut end_ticks: u32) -> u32 {
        if end_ticks < start_ticks {
            end_ticks += Self::TIMER_LIMIT;
        }

        end_ticks - start_ticks
    }
}

#[derive(Clone, Copy)]
pub struct RTCInstant {
    ticks: u32,
}

impl RTCInstant {
    pub fn millis_since(&self, other: &RTCInstant) -> usize {
        (RTC::duration(other.ticks, self.ticks) as usize * 1000) / 32768
    }

    pub fn micros_since(&self, other: &RTCInstant) -> usize {
        // = ticks * (1000000 / 32768)
        const REDUCTION: usize = 32;
        (RTC::duration(other.ticks, self.ticks) as usize * (1000000 / REDUCTION))
            / (32768 / REDUCTION)
    }

    pub fn zero() -> Self {
        Self { ticks: 0 }
    }

    pub fn add_seconds(&self, seconds: f32) -> Self {
        let ticks = (seconds * 32768.0) as u32;
        Self {
            ticks: self.ticks + ticks,
        }
    }
}
