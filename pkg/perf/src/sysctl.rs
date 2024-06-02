use common::errors::*;

/// Verifies that the OS supports and allows unpriveleged users to collect
/// performance events for userspace and kernel activity.
///
/// The current perf_event_paranoid value must be <= 1
/// This can be checked via:
///   cat /proc/sys/kernel/perf_event_paranoid
///
/// It can be temporarily set with:
///   sudo sysctl kernel.perf_event_paranoid=1
///
/// To make the above change permanent, create /etc/sysctl.d/80-perf.conf and
/// add to it:
///   kernel.perf_event_paranoid=1
pub fn check_perf_events_supported() -> Result<()> {
    let contents = match sys::blocking_read_to_string("/proc/sys/kernel/perf_event_paranoid") {
        Ok(v) => v,
        Err(e) => {
            if let Some(e) = e.downcast_ref::<sys::Errno>() {
                if *e == sys::Errno::ENOENT {
                    return Err(err_msg("System does not support perf_event_open"));
                }
            }

            return Err(e);
        }
    };

    let value = contents.trim().parse::<i32>()?;

    if value > 1 {
        return Err(err_msg("perf_event_paranoid must be <= 1"));
    }

    Ok(())
}
