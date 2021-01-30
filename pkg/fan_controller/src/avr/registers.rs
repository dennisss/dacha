use core::ptr::{read_volatile, write_volatile};

pub const MCUCR: *mut u8 = 0x35 as *mut u8;

pub const EIFR: *mut u8 = 0x3c as *mut u8;
pub const EIMSK: *mut u8 = 0x3d as *mut u8;
pub const EECR: *mut u8 = 0x3f as *mut u8;
pub const EEDR: *mut u8 = 0x40 as *mut u8;
pub const EEARL: *mut u8 = 0x41 as *mut u8;
pub const EEARH: *mut u8 = 0x42 as *mut u8;

pub const TCCR0A: *mut u8 = 0x44 as *mut u8;
pub const TCCR0B: *mut u8 = 0x45 as *mut u8;
pub const TCNT0: *mut u8 = 0x46 as *mut u8;
pub const OCR0A: *mut u8 = 0x47 as *mut u8;

pub const PLLCSR: *mut u8 = 0x49 as *mut u8;
pub const PLLFRQ: *mut u8 = 0x52 as *mut u8;

pub const CLKPR: *mut u8 = 0x61 as *mut u8;

pub const EICRA: *mut u8 = 0x69 as *mut u8;
pub const EICRB: *mut u8 = 0x6A as *mut u8;
pub const TIMSK0: *mut u8 = 0x6E as *mut u8;
pub const ADCL: *mut u8 = 0x78 as *mut u8;
pub const ADCH: *mut u8 = 0x79 as *mut u8;
pub const ADCSRA: *mut u8 = 0x7A as *mut u8;
pub const ADCSRB: *mut u8 = 0x7B as *mut u8;
pub const ADMUX: *mut u8 = 0x7C as *mut u8;
pub const DIDR2: *mut u8 = 0x7D as *mut u8;
pub const DIDR0: *mut u8 = 0x7E as *mut u8;
pub const DIDR1: *mut u8 = 0x7F as *mut u8;

pub const UCSR1A: *mut u8 = 0xC8 as *mut u8;
pub const UCSR1B: *mut u8 = 0xC9 as *mut u8;
pub const UCSR1C: *mut u8 = 0xCA as *mut u8;
pub const UCSR1D: *mut u8 = 0xCB as *mut u8;
pub const UBRR1L: *mut u8 = 0xCC as *mut u8;
pub const UBRR1H: *mut u8 = 0xCD as *mut u8;
pub const UDR1: *mut u8 = 0xCE as *mut u8;

// TODO: Configure these?
pub const UHWCON: *mut u8 = 0xD7 as *mut u8;
pub const USBCON: *mut u8 = 0xD8 as *mut u8;
pub const USBSTA: *mut u8 = 0xD9 as *mut u8;

pub const UDCON: *mut u8 = 0xE0 as *mut u8;
pub const UDINT: *mut u8 = 0xE1 as *mut u8;
pub const UDIEN: *mut u8 = 0xE2 as *mut u8;
pub const UDADDR: *mut u8 = 0xE3 as *mut u8;

// Read only frame information
pub const UDFNUML: *mut u8 = 0xE4 as *mut u8;
pub const UDFNUMH: *mut u8 = 0xE5 as *mut u8;
pub const UDMFN: *mut u8 = 0xE6 as *mut u8;

pub const UEINTX: *mut u8 = 0xE8 as *mut u8;
pub const UENUM: *mut u8 = 0xE9 as *mut u8;
pub const UERST: *mut u8 = 0xEA as *mut u8;
pub const UECONX: *mut u8 = 0xEB as *mut u8;
pub const UECFG0X: *mut u8 = 0xEC as *mut u8;
pub const UECFG1X: *mut u8 = 0xED as *mut u8;
pub const UESTA0X: *mut u8 = 0xEE as *mut u8;
pub const UESTA1X: *mut u8 = 0xEF as *mut u8;
pub const UEIENX: *mut u8 = 0xF0 as *mut u8;
pub const UEDATX: *mut u8 = 0xF1 as *mut u8;
pub const UEBCLX: *mut u8 = 0xF2 as *mut u8;
pub const UEBCHX: *mut u8 = 0xF3 as *mut u8;
pub const UEINT: *mut u8 = 0xF4 as *mut u8;

#[cfg(target_arch = "avr")]
#[inline(always)]
pub unsafe fn avr_read_volatile(addr: *const u8) -> u8 {
    read_volatile(addr)
}

#[cfg(target_arch = "avr")]
#[inline(always)]
pub unsafe fn avr_write_volatile(addr: *mut u8, value: u8) {
    write_volatile(addr, value);
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn avr_read_volatile(addr: *const u8) -> u8 {
    let index: usize = core::mem::transmute(addr);
    REGISTERS_BUFFER[index]
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn avr_write_volatile(addr: *mut u8, value: u8) {
    let index: usize = core::mem::transmute(addr);
    REGISTERS_BUFFER[index] = value;
}

#[cfg(target_arch = "x86_64")]
static mut REGISTERS_BUFFER: [u8; 256] = [0; 256];
