use common::errors::*;
use file::{LocalFile, LocalPath};

use sys::bindings::{ptp_pin_function, ptp_perout_request, ptp_pin_desc};

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
    pub fn configure_pps_output(&mut self) -> Result<()> {
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

}