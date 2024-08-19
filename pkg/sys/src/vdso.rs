use crate::bindings::clockid_t;
use crate::Errno;
use crate::{c_int, kernel};

// int clock_gettime(clockid_t clockid, struct timespec *tp);

/*

    vdso_clock_gettime = (vgettime_t)dlsym(vdso, "__vdso_clock_gettime");
    if (!vdso_clock_gettime)
        vdso_clock_gettime = (vgettime_t)dlsym(vdso, "__kernel_clock_gettime");
    if (!vdso_clock_gettime)
        pr_err("Warning: failed to find clock_gettime in vDSO\n");

}
*/

extern "C" {
    fn __vdso_clock_gettime(clockid: clockid_t, tp: *mut kernel::timespec) -> i64;

}

pub fn clock_gettime(clockid: u32) -> Result<kernel::timespec, Errno> {
    let mut time = kernel::timespec::default();
    let val = unsafe { __vdso_clock_gettime(clockid as i32, &mut time) };
    if val != 0 {
        Err(Errno(-val))
    } else {
        Ok(time)
    }
}

// __vdso_clock_gettime
