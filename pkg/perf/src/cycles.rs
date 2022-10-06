use std::fs::File;
use std::os::unix::io::{AsRawFd, FromRawFd};

use common::errors::*;
use sys::bindings::*;

/// Tracker for measuring the number of CPU cycles executed in a single thread.
pub struct CPUCycleTracker {
    file: File,
}

impl CPUCycleTracker {
    pub fn create() -> Result<Self> {
        let mut attr = perf_event_attr::default();
        attr.type_ = perf_type_id::PERF_TYPE_HARDWARE as u32;
        attr.size = core::mem::size_of::<perf_event_attr>() as u32;
        attr.config = perf_hw_id::PERF_COUNT_HW_CPU_CYCLES as u64;
        attr.set_disabled(0); // Start counting right away.
        attr.set_exclude_kernel(1);
        attr.set_exclude_hv(1);
        attr.set_exclude_idle(1);

        let file = unsafe {
            File::from_raw_fd(sys::perf_event_open(
                &attr,
                0,
                -1,
                -1,
                PERF_FLAG_FD_CLOEXEC.into(),
            )?)
        };

        Ok(Self { file })
    }

    pub fn total_cycles(&mut self) -> Result<u64> {
        let mut buf = [0u8; 8];
        unsafe { sys::read(self.file.as_raw_fd(), buf.as_mut_ptr(), buf.len()) }?;
        Ok(u64::from_ne_bytes(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    #[test]
    fn cycles_test() -> Result<()> {
        // Mainly sanity checking that the number of cycles looks ok.

        let mut tracker = CPUCycleTracker::create()?;

        let mut cycles = vec![];

        for i in 0..10 {
            let n = tracker.total_cycles()?;
            cycles.push(n);
        }

        for i in 1..cycles.len() {
            println!("Min period: {}", cycles[i] - cycles[i - 1]);
            assert!(cycles[i] > cycles[i - 1]);
        }

        let start = tracker.total_cycles()?;

        let sampling_secs = 0.2;
        crate::busy::busy_loop(Duration::from_secs_f64(sampling_secs));

        let end = tracker.total_cycles()?;

        let mut cpu_frequency = ((end - start) as f64) / (sampling_secs * 1.0e9);
        println!("CPU Frequency: {:0.2}GHz", cpu_frequency);

        assert!(cpu_frequency > 1.2 && cpu_frequency < 6.0);

        Ok(())
    }
}
