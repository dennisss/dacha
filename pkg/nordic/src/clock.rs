use core::arch::asm;

use peripherals::raw::clock::CLOCK;
use peripherals::raw::register::RegisterRead;

pub fn init_high_freq_clk(clock: &mut CLOCK) {
    // Init HFXO (must be started to use RADIO)
    clock.events_hfclkstarted.write_notgenerated();
    clock.tasks_hfclkstart.write_trigger();

    while clock.events_hfclkstarted.read().is_notgenerated() {
        unsafe { asm!("nop") };
    }
}

pub fn init_low_freq_clk(clock: &mut CLOCK) {
    // NOTE: This must be initialized to use the RTCs.

    // TODO: Must unsure the clock is stopped before changing the source.
    // ^ But clock can only be stopped if clock is running.

    // Use XTAL
    clock
        .lfclksrc
        .write_with(|v| v.set_src_with(|v| v.set_xtal()));

    // Start the clock.
    clock.tasks_lfclkstart.write_trigger();

    while clock.lfclkstat.read().state().is_notrunning() {
        unsafe { asm!("nop") };
    }
}
