/// Enabling FPU per:
/// https://developer.arm.com/documentation/ddi0439/b/Floating-Point-Unit/FPU-Programmers-Model/Enabling-the-FPU?lang=en
///
/// It seems like this must be done after the clocks?
pub fn enable_fpu() {
    const CPACR: *mut u32 = 0xE000ED88 as *mut u32;

    unsafe {
        let mut v = unsafe { core::ptr::read_volatile(CPACR) };

        // Setting both CP00 and CP11 to 0b11. 0b00 disables it.
        v |= (0b11 << 20) | (0b11 << 22);

        core::ptr::write_volatile(CPACR, v);
    }
}
