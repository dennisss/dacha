use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

pub fn idle_counter_value() -> u32 {
    COUNTER.load(Ordering::Relaxed)
} 

pub fn idle_loop() -> ! {
    unsafe { idle_loop_inner(&COUNTER) }
}

/// Google Gemini wrote this function. Some comments from me:
/// - It should take 7 CPU cycles to run.
/// - Running from run currently doesn't work (probably due to THUMB)
/// - It ACTUALLY takes 6 cycles to run since the 'str' only takes 1
///   cycle and the second cycle is pipelined with then next instruction
///   to complete the RAM write.
/// - I added another 4 noops to the code so that the overall time becomes
///   10 cycles. This simplifies the math and reduces the risk of an
///   interrupt happening right after the 'str' and having one extra cycle
///   wasted on waiting for the prior write.
///
/// -- AI PART --
///
/// Runs an infinite loop that increments the counter.
///
/// This function DOES NOT return. It should be placed at the end of main().
/// The counter must be aligned to 4 bytes (Rust u32 standard).
#[inline(never)]
// #[link_section = ".data"] // Run from RAM for deterministic timing (0 wait states)
unsafe fn idle_loop_inner(counter: &AtomicU32) -> ! {
    let ptr = counter.as_ptr();

    loop {
        asm!(
            // Align code to 16-byte boundary to ensure the branch target 
            // interacts with the prefetch buffer consistently.
            ".align 4", 
            
            "2:",
            // 1. Load (2 cycles)
            "ldr {tmp}, [{ptr}]",
            
            // 2. Add (1 cycle)
            "add {tmp}, #1",
            
            // 3. Store (2 cycles)
            // Since only this loop writes, and interrupts only read, 
            // a standard 32-bit aligned STR is atomic enough on Cortex-M4.
            "str {tmp}, [{ptr}]",
            
            "nop",
            "nop",
            "nop",
            "nop",

            // 4. Branch (2 cycles)
            "b 2b",

            ptr = in(reg) ptr,
            tmp = out(reg) _,
            options(nostack)
        );
    }
}