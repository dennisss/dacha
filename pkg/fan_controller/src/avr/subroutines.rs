/*
// Runs in a never ending loop while incrementing the value of an integer given
// as an argument.
//
// The counter will be incremented every 32 cycles. This is meant to run in the
// main() function will actually operations happening in interrupt handlers.
// This means that a host can retrieve the value of the counter at two different
// points in time to determine the utilization.
//
// Based on instruction set timing in:
// https://ww1.microchip.com/downloads/en/DeviceDoc/AVR-Instruction-Set-Manual-DS40002198A.pdf
// using the ATmega32u4 (AVRe+)
//
// Args:
//   addr: *mut u32 : Stored in r24 (low) and r25 (high). This is the address of
//                    integer that should be incremented on each loop cycle.
//
// Internally uses:
// - r18-r21 to store the current value of the counter.
// - Z (r30, r31) to store working address.
// - r22: stores a 0 value.
//
// Never returns.
#[cfg(target_arch = "avr")]
global_asm!(
    r#"
    .global avr_idle_loop
avr_idle_loop:
    ; NOTE: We only use call cloberred registers so don't need to push anything
    ; to the stack

    ; Initialize count to zero
    ; NOTE: We assume that the value at the counter has already been
    ; initialized to 0
    clr r18
    clr r19
    clr r20
    clr r21

    ; r22 will always be zero
    clr r22

avr_idle_loop_start:
    ; These use 15 cycles
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop
    nop

    ; Add 1 to the 32bit counter
    inc r18 ; (1 cycle)
    adc r19, r22 ; += 0 + C (1 cycle)
    adc r20, r22 ; += 0 + C (1 cycle)
    adc r21, r22 ; += 0 + C (1 cycle)

    ; Load Z with the address of the counter (first argument to the function)
    movw r30, r24 ; (1 cycle)

    ; Store into memory
    cli ; (1 cycle)
    st Z+, r18 ; (2 cycles)
    st Z+, r19 ; (2 cycles)
    st Z+, r20 ; (2 cycles)
    st Z+, r21 ; (2 cycles)
    sei ; (1 cycle)

    ; Loop
    rjmp avr_idle_loop_start ; (2 cycles)
"#
);

#[cfg(target_arch = "avr")]
extern "C" {
    pub fn avr_idle_loop(addr: *mut u32) -> !;
}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
pub extern "C" fn avr_idle_loop(addr: *mut u32) -> ! {
    loop {}
}
*/
