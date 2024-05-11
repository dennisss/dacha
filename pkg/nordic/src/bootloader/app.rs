use core::arch::asm;
use core::ptr::read_volatile;

use common::register::{RegisterRead, RegisterWrite};
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
use nordic_proto::nordic::BootloaderParams;
use peripherals::raw::nvic::NVIC_VTOR;
use peripherals::raw::power::resetreas::RESETREAS_VALUE;

use crate::bootloader::flash::*;
use crate::reset::ResetState;

pub enum EnterBootloaderReason {
    Unknown = 0,
    NoApplication = 1,
    ApplicationOverflow = 2,
    BadApplicationChecksum = 3,
    ResetPin = 4,
    ApplicationRequested = 5,
}

// NOTE: This code should not depend on any peripherals being initialized as we
// want to run it as early in the boot process as possible and not initialize
// any peripherals to a non-initial state which the application may not be able
// to handle.
pub fn maybe_enter_application(params: &BootloaderParams) -> EnterBootloaderReason {
    let mut peripherals = peripherals::raw::Peripherals::new();

    let mut reason = EnterBootloaderReason::Unknown;

    let reset_reason = peripherals.power.resetreas.read();
    let reset_state = ResetState::from_value(peripherals.power.gpregret.read());

    peripherals
        .power
        .gpregret
        .write(ResetState::Default.to_value());

    // Clear by setting to all 1's
    peripherals
        .power
        .resetreas
        .write(RESETREAS_VALUE::from_raw(0xffffffff));

    let mut should_enter_bootloader = false;

    // Enter the bootloader if the reset was triggered by the RESET pin.
    // NOTE: Because of erata #136, if the reset pin bit is set, all other bits in
    // RESETREAS must be ignored.
    should_enter_bootloader |= reset_reason.resetpin().is_detected();
    if should_enter_bootloader {
        reason = EnterBootloaderReason::ResetPin;
    }

    match reset_state {
        ResetState::Default => {}
        ResetState::EnterBootloader => {
            should_enter_bootloader = true;
            reason = EnterBootloaderReason::ApplicationRequested;
        }
        ResetState::EnterApplication => {
            should_enter_bootloader = false;
        }
        ResetState::Unknown(_) => {}
    }

    // We can only enter the application if it is valid.
    if !should_enter_bootloader {
        if !has_valid_application(params, &mut reason) {
            return reason;
        }

        enter_application();
    }

    reason
}

fn has_valid_application(params: &BootloaderParams, reason: &mut EnterBootloaderReason) -> bool {
    if params.application_size() == 0 || params.num_flashes() == 0 {
        *reason = EnterBootloaderReason::NoApplication;
        return false;
    }

    let mut app_data = unsafe { application_code_data() };
    if (params.application_size() as usize) > app_data.len() {
        *reason = EnterBootloaderReason::ApplicationOverflow;
        return false;
    }

    app_data = &app_data[..(params.application_size() as usize)];

    let expected_sum = {
        let mut hasher = CRC32CHasher::new();
        hasher.update(app_data);
        hasher.finish_u32()
    };

    if expected_sum != params.application_crc32c() {
        *reason = EnterBootloaderReason::BadApplicationChecksum;
        return false;
    }

    true
}

fn enter_application() {
    // See https://developer.arm.com/documentation/ka001423/1-0

    // TODO: Do this as early as possible (ideally in main() before peripherals are
    // loaded).
    unsafe {
        let sp = read_volatile(APPLICATION_CODE_OFFSET as *mut u32);
        let ep = read_volatile((APPLICATION_CODE_OFFSET + 4) as *mut u32);

        core::ptr::write_volatile(NVIC_VTOR, APPLICATION_CODE_OFFSET);
        asm!(
            "mov sp, {sp}",
            "bx {ep}",
            sp = in(reg) sp,
            ep = in(reg) ep,
        )
    };
}
