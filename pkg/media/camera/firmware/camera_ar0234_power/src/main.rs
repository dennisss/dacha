// Firmware for implementing the power up and down sequence for the AR0234
//
// See pkg/media/camera/boards/camera_ar0234/index.md for how to flash this.
//
// - LDOs can take up to 1ms to reach full power
// - The crystal will take up to 5ms to start up.
// - After the 1ms reset pulse is sent, ~7ms (~160000 EXTCLK cycles) are
//   required for initialization before the sensor is ready for
//   communication.

#![feature(asm_experimental_arch)]
#![no_std]
#![no_main]

use core::panic::PanicInfo;

const PORTA_DIR: *mut u8    = 0x0400 as *mut u8;
const PORTA_DIRSET: *mut u8 = 0x0401 as *mut u8;
const PORTA_DIRCLR: *mut u8 = 0x0402 as *mut u8;
const PORTA_OUT: *mut u8    = 0x0404 as *mut u8;
const PORTA_OUTSET: *mut u8 = 0x0405 as *mut u8;
const PORTA_OUTCLR: *mut u8 = 0x0406 as *mut u8;
const VPORTA_IN: *mut u8    = 0x0002 as *mut u8;

const PIN_ENABLE_3V3: u8    = 1 << 0;
const PIN_RESET_1V8: u8     = 1 << 1;
const PIN_ENABLE_1V8: u8    = 1 << 2;
const PIN_EXTCLK_ENABLE: u8 = 1 << 3;
const PIN_ENABLE_2V8: u8    = 1 << 6;
const PIN_ENABLE_1V2: u8    = 1 << 7;

const CPU_CCP: *mut u8 = 0x0034 as *mut u8;
const CLKCTRL_MCLKCTRLB: *mut u8 = 0x0061 as *mut u8;

// Clock speed 1MHz setup
fn setup_clock_1mhz() {
    unsafe {
        // Unlock MCLKCTRLB using CCP
        core::ptr::write_volatile(CPU_CCP, 0xD8);
        // Prescaler = 16 (for 16MHz base) -> 1MHz. PDIV=0x3. PEN=1.
        // (0x3 << 1) | 1 = 0x07
        core::ptr::write_volatile(CLKCTRL_MCLKCTRLB, 0x07);
    }
}

#[inline(never)]
fn delay_ms(mut ms: u16) {
    while ms > 0 {
        unsafe {
            core::arch::asm!(
                "1:",
                "ldi r24, 249", // 1 cycle
                "2:",
                "nop",          // 1 cycle
                "dec r24",      // 1 cycle
                "brne 2b",      // 2 cycles
                out("r24") _,
            );
        }
        ms -= 1;
    }
}

#[inline(always)]
fn delay_10us() {
    unsafe {
        core::arch::asm!(
            "nop", "nop", "nop", "nop", "nop",
            "nop", "nop", "nop", "nop", "nop",
        );
    }
}

fn read_debounced(target_state: bool) -> bool {
    let mut matches = 0;
    
    for _ in 0..20 {
        let val = unsafe { core::ptr::read_volatile(VPORTA_IN) };
        let state = (val & PIN_ENABLE_3V3) != 0;
        
        if state == target_state {
            matches += 1;
        }
        delay_10us();
    }
    
    matches >= 16
}

fn power_on_sequence() {
    unsafe {
        core::ptr::write_volatile(PORTA_OUTSET, PIN_ENABLE_2V8);
        delay_ms(1);

        core::ptr::write_volatile(PORTA_OUTSET, PIN_ENABLE_1V8);
        delay_ms(1);

        core::ptr::write_volatile(PORTA_OUTSET, PIN_ENABLE_1V2);

        // Wait 1ms for all the LDOs to stabilize.
        delay_ms(1);

        // Externally pulled up so set to High-Z to enable the clock.
        core::ptr::write_volatile(PORTA_DIRCLR, PIN_EXTCLK_ENABLE); // Set as input.
        
        // Wait for clock to stabilize.
        delay_ms(10);

        // Reset needs to be pulsed low for at least 1ms
        core::ptr::write_volatile(PORTA_OUTCLR, PIN_RESET_1V8); // Drive low
        core::ptr::write_volatile(PORTA_DIRSET, PIN_RESET_1V8); // Set ad output
        delay_ms(2);
        core::ptr::write_volatile(PORTA_DIRCLR, PIN_RESET_1V8); // Set as input

        // Fix time for internal initialization 
        delay_ms(10);
    }
}

fn power_off_sequence() {
    unsafe {
        core::ptr::write_volatile(PORTA_OUTCLR, PIN_EXTCLK_ENABLE); // Drive low
        core::ptr::write_volatile(PORTA_DIRSET, PIN_EXTCLK_ENABLE); // Set as output
        
        delay_ms(1);

        core::ptr::write_volatile(PORTA_OUTCLR, PIN_ENABLE_1V2);
        delay_ms(1);
        
        core::ptr::write_volatile(PORTA_OUTCLR, PIN_ENABLE_1V8);
        delay_ms(1);
        
        core::ptr::write_volatile(PORTA_OUTCLR, PIN_ENABLE_2V8);

        // Prevent immediately powering back on (datasheet recommends at least 100ms).
        delay_ms(100);
    }
}

#[no_mangle]
extern "C" fn main() -> ! {
    setup_clock_1mhz();

    unsafe {
        // Ensure all pins have explicitly well-defined initial OUT values (all LOW)
        core::ptr::write_volatile(PORTA_OUT, 0x00);
        
        // Explicitly set DIR for all pins (1 = OUTPUT, 0 = INPUT).
        // PA0 (ENABLE_3V3) and PA1 (RESET_1V8) are INPUTs.
        core::ptr::write_volatile(PORTA_DIR, 
            PIN_ENABLE_1V8 | PIN_EXTCLK_ENABLE | PIN_ENABLE_2V8 | PIN_ENABLE_1V2
        );
    }

    delay_ms(500);

    let mut is_powered_on = false;

    loop {
        let val = unsafe { core::ptr::read_volatile(VPORTA_IN) };
        let enable_state = (val & PIN_ENABLE_3V3) != 0;

        if !is_powered_on && enable_state {
            if read_debounced(true) {
                power_on_sequence();
                is_powered_on = true;
            }
        } else if is_powered_on && !enable_state {
            if read_debounced(false) {
                power_off_sequence();
                is_powered_on = false;
            }
        }
    }
}

#[no_mangle]
extern "C" fn exit(_status: i16) -> ! {
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
