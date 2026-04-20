use std::time::Duration;

use common::errors::*;
use file::{LocalFile, LocalPath};

use sys::bindings::{ptp_pin_function, ptp_perout_request, ptp_pin_desc, ptp_clock_time};

use crate::ioctl::*;

pub struct PTPDevice {
    fd: LocalFile
}


impl PTPDevice {
    pub fn open_default() -> Result<Self> {
        let mut fd = file::LocalFile::open_with_options(
            "/dev/ptp0",
            file::LocalFileOpenOptions::new().write(true),
        )?;

        Ok(Self {
            fd
        })
    }

    /// Configures the first pin tied to the PTP device to output a 1Hz
    /// pulse.
    ///
    /// This is the equivalent of running the following commands:
    ///     sudo ./testptp -d /dev/ptp0 -L0,2
    ///     sudo ./testptp -d /dev/ptp0 -p 1000000000
    pub fn configure_pps_output(&self) -> Result<()> {
        let mut desc = ptp_pin_desc::default();
        desc.index = 0;
        desc.chan = 0;
        desc.func = ptp_pin_function::PTP_PF_PEROUT as u32;
        unsafe { ptp_pin_setfunc(self.fd.as_raw_fd(), &desc) }?;

        let mut req = ptp_perout_request::default();
        req.period.sec = 1;
        req.period.nsec = 0;
        unsafe { ptp_perout_request2(self.fd.as_raw_fd(), &req) }?;

        Ok(())
    }

    pub fn get_time(&self) -> Result<sys::kernel::timespec> {
        Ok(sys::ClockId::from_fd(unsafe { self.fd.as_raw_fd() }).get_time()?)
    }

    pub fn clock(&self) -> sys::ClockId {
        sys::ClockId::from_fd(unsafe { self.fd.as_raw_fd() })
    }

    #[cfg(all(target_arch = "aarch64"))]
    pub fn offset_to(&self, other_clock_id: sys::ClockId) -> Result<PTPClockOffset> {

        let mut data = sys::bindings::ptp_sys_offset_extended::default();
        data.n_samples = 3;
        data.clockid = other_clock_id.0;

        unsafe {
            crate::ioctl::ptp_sys_offset_extended2(self.fd.as_raw_fd(), &mut data)
        }?;

        let mut best_offset = PTPClockOffset {
            ptp_time: Duration::ZERO,
            other_time: Duration::ZERO,
            rtt: Duration::ZERO
        };

        for i in 0..3 {
            let other_time = to_dur(&data.ts[i][0]);
            let ptp_time = to_dur(&data.ts[i][1]);
            let other_time2 = to_dur(&data.ts[i][2]);
            let rtt = other_time2 - other_time;

            if i == 0 || rtt < best_offset.rtt {
                best_offset = PTPClockOffset {
                    ptp_time,
                    other_time: other_time + (rtt / 2),
                    rtt
                };
            }
        }

        Ok(best_offset)
    }

    /// For older kernels that don't have 'ptp_sys_offset_extended2'
    #[cfg(not(target_arch = "aarch64"))]
    pub fn offset_to(&self, other_clock_id: sys::ClockId) -> Result<PTPClockOffset> {
        let mut best_offset = PTPClockOffset {
            ptp_time: Duration::ZERO,
            other_time: Duration::ZERO,
            rtt: Duration::ZERO
        };
        
        for i in 0..3 {
            let other_time = other_clock_id.get_time()?.into();
            let ptp_time = self.get_time()?.into();
            let other_time2: Duration = other_clock_id.get_time()?.into();
            let rtt = other_time2 - other_time;

            if i == 0 || rtt < best_offset.rtt {
                best_offset = PTPClockOffset {
                    ptp_time,
                    other_time: other_time + (rtt / 2),
                    rtt
                };
            }
        }

        Ok(best_offset)
    }
}

fn to_dur(t: &ptp_clock_time) -> Duration {
    Duration::from_secs(t.sec as u64) + Duration::from_nanos(t.nsec as u64)
}

#[derive(Clone, Debug)]
pub struct PTPClockOffset {
    pub ptp_time: Duration,
    
    /// NOTE: This already includes compensation for half the RTT.
    pub other_time: Duration,

    pub rtt: Duration,
}
