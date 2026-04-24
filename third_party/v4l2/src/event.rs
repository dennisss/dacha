use std::time::Duration;

use crate::bindings::*;

#[derive(Clone)]
pub struct Event {
    pub(crate) raw: v4l2_event
}

impl Event {

    pub fn typ(&self) -> EventType {
        EventType::from_value(self.raw.type_)
    }

    pub fn sequence(&self) -> u32 {
        self.raw.sequence
    }

    /// This uses the same clock as clock_gettime(CLOCK_MONOTONIC).
    pub fn monotonic_timestamp(&self) -> Duration {
        let t = self.raw.timestamp;
        Duration::from_secs(t.tv_sec as u64) + Duration::from_nanos(t.tv_nsec as u64)
    }

    pub fn data(&self) -> EventData {
        match self.typ() {
            EventType::FRAME_SYNC => {
                let data = unsafe { &self.raw.u.frame_sync };
                EventData::FrameSync(FrameSyncEvent {
                    frame_sequence: data.frame_sequence
                })
            }
            EventType::Unknown(_) => {
                EventData::Unknown
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum EventData {
    FrameSync(FrameSyncEvent),
    Unknown
}

#[derive(Clone, Debug)]
pub struct FrameSyncEvent {
    pub frame_sequence: u32
}

enum_def_with_unknown!(EventType u32 =>
    FRAME_SYNC = V4L2_EVENT_FRAME_SYNC
);




