use common::register::RegisterWrite;
use peripherals::raw::power::POWER;

// Well defined values for the GPREGRET flag.
enum_def_with_unknown!(ResetState u32 =>
    // The default state observed from the first power up or a normal reset.
    Default = 0,

    EnterBootloader = 1,

    EnterApplication = 2
);

/// Restart the MCU and force entering the bootloader.
pub fn reset_to_bootloader() {
    let mut power = unsafe { POWER::new() };
    power.gpregret.write(ResetState::EnterBootloader.to_value());
    peripherals::raw::reset();
}

pub fn reset_to_application() {
    let mut power = unsafe { POWER::new() };
    power
        .gpregret
        .write(ResetState::EnterApplication.to_value());
    peripherals::raw::reset();
}
