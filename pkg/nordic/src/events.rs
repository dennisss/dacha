use core::arch::asm;

/// Call this after clearing event or interrupt enable/disable registers to
/// ensure that the events don't immediately retrigger an interrupt and to allow
/// future tasks to immediately trigger new interactions of the events.
///
/// See https://docs.nordicsemi.com/bundle/ps_nrf52840/page/peripheral_interface.html#d834e244
#[inline(always)]
pub fn flush_events_clear() {
    unsafe {
        asm!("nop");
        asm!("nop");
        asm!("nop");
        asm!("nop");
    }
}
