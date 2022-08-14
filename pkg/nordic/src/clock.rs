use core::arch::asm;

use peripherals::raw::clock::CLOCK;
use peripherals::raw::register::RegisterRead;

/*
Need a reference counting system for enabling/disabling these clocks.
*/

pub fn init_high_freq_clk(clock: &mut CLOCK) {
    // Init HFXO (must be started to use RADIO)
    clock.events_hfclkstarted.write_notgenerated();
    clock.tasks_hfclkstart.write_trigger();

    while clock.events_hfclkstarted.read().is_notgenerated() {
        unsafe { asm!("nop") };
    }
}

/*
TODO: If no external crystal is used, use the LFRC
TODO: Perform initial calibration from the HFXO

LFXO run current is 0.23 uA
LFRC run current is 0.7 uA
LFRC run current (ULP) is 0.3 uA

HFXO run current is 80 - 800 uA depending on the crystal
    -> So more valuable to disable this when not in use.

*/

pub enum LowFrequencyClockSource {
    /// External component
    CrystalOscillator,

    /// Internal
    /// TODO: Support ultra-low power mode?
    RCOscillator,
}

pub fn init_low_freq_clk(source: LowFrequencyClockSource, clock: &mut CLOCK) {
    // NOTE: This must be initialized to use the RTCs.

    // TODO: Must unsure the clock is stopped before changing the source.
    // ^ But clock can only be stopped if clock is running.

    match source {
        LowFrequencyClockSource::CrystalOscillator => {
            clock
                .lfclksrc
                .write_with(|v| v.set_src_with(|v| v.set_xtal()));
        }
        LowFrequencyClockSource::RCOscillator => {
            clock
                .lfclksrc
                .write_with(|v| v.set_src_with(|v| v.set_rc()));
        }
    }

    // Start the clock.
    clock.tasks_lfclkstart.write_trigger();

    while clock.lfclkstat.read().state().is_notrunning() {
        unsafe { asm!("nop") };
    }
}
