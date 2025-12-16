use common::fixed::vec::FixedVec;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode, I2CTransfer
};

use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;
use crate::timer::*;
use crate::ppi::*;
use crate::radio::*;


pub struct RadioEntry {
    radio: Radio,
    time_sync: RadioTimeSyncer,
}

impl RadioEntry {
    pub fn create(
        mut radio: Radio,
        timer: &'static Timer,
        ppi: &mut PPIChannels
    ) -> Option<Self> {
        let time_sync = match RadioTimeSyncer::create(&mut radio, timer, ppi) {
            Some(v) => v,
            None => return None
        };

        Some(Self {
            radio,
            time_sync
        })
    }

    pub fn into_inner(self) -> Radio {
        self.radio
    }

}


// TODO: Replace with the TimedEvent
struct RadioTimeSyncer {
    timer_channel: TimerChannel<'static>,
    ppi_channel: PPIChannel,
}

impl RadioTimeSyncer {

    pub fn create(
        radio: &mut Radio,
        timer: &'static Timer,
        ppi: &mut PPIChannels
    ) -> Option<Self> {

        let mut timer_channel = match timer.new_channel() {
            Some(v) => v,
            None => return None
        };

        let mut ppi_channel = match ppi.new_channel(
            radio.end_event(),
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
}

