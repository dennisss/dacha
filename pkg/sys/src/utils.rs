use crate::Errno;

/// Runs a function unless we receive a non-EINTR error (or a success).
///
/// EINTR should always indicate that the program was interrupted by signal
/// delivery during a syscall.
#[inline(always)]
pub(crate) fn retry_interruptions<T, F: Fn() -> Result<T, Errno>>(f: F) -> Result<T, Errno> {
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                if e == Errno::EINTR {
                    continue;
                }

                return Err(e);
            }
        }
    }
}
