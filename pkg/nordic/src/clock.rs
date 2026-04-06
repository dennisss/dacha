use core::arch::asm;

use executor::CriticalSection;
use peripherals::raw::clock::CLOCK;
use common::register::RegisterRead;

/*
Need a reference counting system for enabling/disabling these clocks.
*/

static mut HFCLK_REF_COUNT: usize = 0;

pub fn reference_hfclk() {
    let cs = CriticalSection::new();

    unsafe {
        if HFCLK_REF_COUNT == 0 {
            let mut clock = CLOCK::new();
            init_high_freq_clk(&mut clock);
        }

        HFCLK_REF_COUNT += 1;
    }
}

pub fn unreference_hfclk() {
    let cs = CriticalSection::new();

    unsafe {
        HFCLK_REF_COUNT -= 1;

        if HFCLK_REF_COUNT == 0 {
            let mut clock = CLOCK::new();
            stop_high_freq_clk(&mut clock);
        }
    }
}




fn init_high_freq_clk(clock: &mut CLOCK) {
    // TODO: Appropriately seutp HFXODEBOUNCE.

    // Init HFXO (must be started to use RADIO)
    clock.events_hfclkstarted.write_notgenerated();
    clock.tasks_hfclkstart.write_trigger();

    while clock.events_hfclkstarted.read().is_notgenerated() {
        unsafe { asm!("nop") };
    }
}

fn stop_high_freq_clk(clock: &mut CLOCK) {
    clock.tasks_hfclkstop.write_trigger();
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
    RCOscillator,

    /// Internal (low power, low accuracy)
    ///
    /// TODO: Figure out why this isn't reducing power usage.
    RCOscillatorULP,
}

// TODO: Implement periodic calibration for the RC clock.
pub fn init_low_freq_clk(source: LowFrequencyClockSource, clock: &mut CLOCK) {
    // NOTE: This must be initialized to use the RTCs.

    // TODO: Must unsure the clock is stopped before changing the source.
    // ^ But clock can only be stopped if clock is running.

    if clock.lfclkstat.read().state().is_running() {
        clock.tasks_lfclkstop.write_trigger();
        while clock.lfclkstat.read().state().is_running() { 
            unsafe { asm!("nop") };
        }
    }

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
            clock
                .lfrcmode
                .write_with(|v| v.set_mode_with(|v| v.set_normal()));
        }
        LowFrequencyClockSource::RCOscillatorULP => {
            clock
                .lfclksrc
                .write_with(|v| v.set_src_with(|v| v.set_rc()));            
            clock
                .lfrcmode
                .write_with(|v| v.set_mode_with(|v| v.set_ulp()));
        }
    }

    // Errata 20
    clock.events_lfclkstarted.write_notgenerated();
    unsafe { asm!("nop") };
    unsafe { asm!("nop") };
    unsafe { asm!("nop") };
    unsafe { asm!("nop") };


    // Start the clock.
    clock.tasks_lfclkstart.write_trigger();

    // Errata 20
    // This also catches configuring the wrong clock.
    // (e.g. if you configure XTAL and don't have an XTAL, this will not fire, the clock will
    //  still report as running due to the initial time period using the RC oscillator but this
    //  results in silent glitching later if you really don't have an XTAL).
    while clock.events_lfclkstarted.read().is_notgenerated() {
        unsafe { asm!("nop") };
    }

    while clock.lfclkstat.read().state().is_notrunning() {
        unsafe { asm!("nop") };
    }
}
