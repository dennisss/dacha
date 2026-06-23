use peripherals::raw::EventRegister;

use crate::timer::*;
use crate::ppi::*;


pub struct TimedEvent {
    timer_channel: TimerChannel<'static>,
    ppi_channel: PPIChannel,
}

impl TimedEvent {
    pub fn create(
        event: &EventRegister,
        timer: &'static Timer,
        ppi: &mut PPIChannels
    ) -> Option<Self> {

        let mut timer_channel = match timer.new_channel() {
            Some(v) => v,
            None => return None
        };

        let mut ppi_channel = match ppi.new_channel(
            event,
            timer_channel.capture_task(),
        ) {
            Some(v) => v,
            None => return None
        };

        // Always enabled. Nothing bad happens if we keep it this way.
        ppi_channel.enable();

        Some(Self {
            timer_channel,
            ppi_channel
        })
    }

    /// NOTE: Must be called at least one 16Mhz clock cycle after the event occurred.
    pub fn last_time(&self) -> u32 {
        self.timer_channel.compare_value()
    }
}
