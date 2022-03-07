use core::arch::asm;

/// Call this after clearing event or interrupt enable/disable registers to
/// ensure that the events don't immediately retrigger an interrupt and to allow
/// future tasks to immediately trigger new interactions of the events.
#[inline(always)]
pub fn flush_events_clear() {
    unsafe {
        asm!("nop");
        asm!("nop");
        asm!("nop");
        asm!("nop");
    }
}
