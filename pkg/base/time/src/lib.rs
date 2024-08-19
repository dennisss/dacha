extern crate core;

use core::ops::{Add, Sub};
use std::sync::Mutex;
pub use std::time::Duration;

const NANOS_PER_SECOND: u64 = 1_000_000_000;

// TODO: Also need the last returned time to guarantee things are monotonic.
static mut LAST_TIME_REFERENCE: Mutex<Option<TimeReference>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Time {
    secs: u64,
    nanos: u64,
}

struct TimeState {
    /// Last time returned from Time::now(). Used to guarantee monotonicity.
    last_returned_time: Time,
    // reference_time: Option<>
}

#[derive(Clone)]
struct TimeReference {
    realtime: sys::kernel::timespec,
    boottime: sys::kernel::timespec,
    monotonic: sys::kernel::timespec,
}

impl Time {
    pub fn now() -> Self {
        let raw_time = Self::now_raw();

        let mut last_ref = unsafe { LAST_TIME_REFERENCE.lock().unwrap() };

        if let Some(last_time) = last_ref.as_ref() {
            /*

            Return last_returned_time + (raw_time.boottime - last_time.boottime)
             */

            if raw_time.monotonic.tv_sec - last_time.monotonic.tv_sec > 5 {
                *last_ref = Some(raw_time.clone());
            }
        } else {
            *last_ref = Some(raw_time.clone());

            Self {
                secs: raw_time.realtime.tv_sec,
                nanos: raw_time.realtime.tv_nsec as u32,
            }
        }
    }

    fn now_raw() -> TimeReference {
        let mut realtime = sys::kernel::timespec::default();
        let mut boottime = sys::kernel::timespec::default();
        let mut monotonic = sys::kernel::timespec::default();

        unsafe {
            sys::clock_gettime(sys::bindings::CLOCK_REALTIME as i32, &mut realtime).unwrap();
            sys::clock_gettime(sys::bindings::CLOCK_BOOTTIME as i32, &mut boottime).unwrap();
            sys::clock_gettime(sys::bindings::CLOCK_MONOTONIC as i32, &mut monotonic).unwrap();
        };

        TimeReference {
            realtime,
            boottime,
            monotonic,
        }
    }

    /// NOTE: We don't want this to be public.
    fn from_timespec(time: &sys::kernel::timespec) -> Self {
        Self {
            secs: time.tv_sec,
            nanos: time.tv_nsec,
        }
    }
}

impl Add<Duration> for Time {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self::Output {
        // let mut secs = self.secs +

        todo!()
    }
}

impl Sub<Duration> for Time {
    type Output = Time;

    fn sub(self, rhs: Duration) -> Self::Output {
        todo!()
    }
}

impl Sub<Self> for Time {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Self::Output {
        let mut secs = self.secs - rhs.secs;

        let mut nanos = self.nanos;
        if self.nanos < rhs.nanos {
            secs -= 1;
            nanos += NANOS_PER_SECOND;
        }

        nanos -= rhs.nanos;

        Duration::from_secs(secs) + Duration::from_nanos(nanos)
    }
}
