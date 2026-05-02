use crate::{syscall, bindings, kernel, Errno, c_int};

const CLOCKFD: i32 = 3;

pub struct ClockId(pub bindings::clockid_t);

impl ClockId {
    pub const REALTIME: Self = Self(bindings::CLOCK_REALTIME as i32);

    pub const MONOTONIC: Self = Self(bindings::CLOCK_MONOTONIC as i32);

    // TODO: Consider making this unsafe as we don't know if the file is still open.
    pub fn from_fd(fd: c_int) -> Self {
        let v = (!fd) << 3 | CLOCKFD;
        Self(v)
    }

    pub fn get_time(&self) -> Result<kernel::timespec, Errno> {
        let mut time = kernel::timespec::default();
        unsafe {
            raw::clock_gettime(self.0, &mut time)?;
        }
        Ok(time)
    }

    pub fn set_time(&self, time: &kernel::timespec) -> Result<(), Errno> {
        unsafe {
            raw::clock_settime(self.0, time)
        }
    }

    pub fn get_adjustments(&self) -> Result<ClockAdjustments, Errno> {
        let mut inner = bindings::__kernel_timex::default();
        unsafe {
            raw::clock_adjtime(self.0, &mut inner)?;
        }

        Ok(ClockAdjustments { inner })
    }

    /// NOTE: We will only make changes explicitly marked with the 'set_*' methods
    /// in ClockAdjustments.
    pub fn set_adjustments(&self, v: &ClockAdjustments) -> Result<(), Errno> {
        let mut buf = v.inner.clone();
        unsafe {
            raw::clock_adjtime(self.0, &mut buf)
        }
    }

    pub fn set_offset_secs_f64(&self, offset: f64) -> Result<(), Errno> {
        let mut adj = bindings::__kernel_timex::default();
        adj.modes |= bindings::ADJ_SETOFFSET | bindings::ADJ_NANO;

        let offset_ns = (offset * 1_000_000_000.0).round() as i64;
        let mut sec = offset_ns / 1_000_000_000;
        let mut nsec = offset_ns % 1_000_000_000;
        while nsec < 0 {
            sec -= 1;
            nsec += 1_000_000_000;
        }

        adj.time.tv_sec = sec as _;
        adj.time.tv_usec = nsec as _;

        unsafe {
            raw::clock_adjtime(self.0, &mut adj)
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClockAdjustments {
    inner: bindings::__kernel_timex
}

impl ClockAdjustments {    
    /// 2^16 = 1ppm
    pub fn freq(&self) -> i64 {
        self.inner.freq
    }

    /// TODO: Check that it is in the range (-32768000, +32768000) which is ~ +/- 500ppm
    pub fn set_freq(&mut self, v: i64) {
        self.inner.modes |= bindings::ADJ_FREQUENCY;
        self.inner.freq = v;
    }

}

mod raw {
    use super::*;

    // TODO: Need to use the VDSO versions.
    syscall!(clock_gettime, bindings::SYS_clock_gettime, clockid: bindings::clockid_t, tp: *mut kernel::timespec => Result<()>);

    syscall!(clock_settime, bindings::SYS_clock_settime, clockid: bindings::clockid_t, tp: *const kernel::timespec => Result<()>);

    syscall!(
        clock_adjtime,
        bindings::SYS_clock_adjtime,
        clockid: bindings::clockid_t,
        timex: *mut bindings::__kernel_timex => Result<()>
    );
}

