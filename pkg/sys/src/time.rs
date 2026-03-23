use crate::{syscall, bindings, kernel, Errno, c_int};

const CLOCKFD: i32 = 3;

pub struct ClockId(bindings::clockid_t);

impl ClockId {
    pub const REALTIME: Self = Self(bindings::CLOCK_REALTIME as i32);

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
}

#[derive(Clone, Debug)]
pub struct ClockAdjustments {
    inner: bindings::__kernel_timex
}

impl ClockAdjustments {
    // TODO: Need signed durations for this.
    /*
    pub fn offset(&self) -> Duration {
        if self.inner.status & bindigns::STA_NANO != 0 {
            Duration::from_nanos(self.inner.offset)
        } else {
            Duration::from_micros(self.inner.offset)
        }
    }

    /// TODO: Complain if >0.5 seconds
    pub fn set_offset(&mut self, v: Duration) {
        self.inner.modes |= bindings::ADJ_OFFSET | bindings::ADJ_NANO;
        self.inner.status |= bindings::STA_NANO;
        self.inner.offset = v.as_nanos();
    }
    */
    
    /// 2^16 = 1ppm
    pub fn freq(&self) -> i64 {
        self.inner.freq
    }

    /// TODO: Check that it is in the range (-32768000, +32768000)
    pub fn set_freq(&mut self, v: i64) {
        self.inner.modes |= bindings::ADJ_FREQUENCY;
        self.inner.freq = v;
    }

}

mod raw {
    use super::*;

    syscall!(clock_gettime, bindings::SYS_clock_gettime, clockid: bindings::clockid_t, tp: *mut kernel::timespec => Result<()>);

    syscall!(clock_settime, bindings::SYS_clock_settime, clockid: bindings::clockid_t, tp: *const kernel::timespec => Result<()>);

    syscall!(
        clock_adjtime,
        bindings::SYS_clock_adjtime,
        clockid: bindings::clockid_t,
        timex: *mut bindings::__kernel_timex => Result<()>
    );
}

