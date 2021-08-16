#![feature(llvm_asm)]
#![no_std]
#![no_main]

#[no_mangle]
#[inline(never)]
fn make_num() -> u8 {
    unsafe { llvm_asm!("nop") };

    let mut v = 1;
    for i in 0..100 {
        v += 1
    }

    v
}

struct SimpleValue {
    addr: u16,
    value: Option<u8>,
}

#[no_mangle]
#[inline(never)]
fn make_value() -> SimpleValue {
    SimpleValue {
        addr: make_num() as u16,
        value: Some(make_num()),
    }
}

#[no_mangle]
pub extern "C" fn main() {
    make_value();
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    abort();
}

#[no_mangle]
pub extern "C" fn abort() -> ! {
    loop {}
}

#[cfg(target_arch = "avr")]
#[no_mangle]
pub extern "C" fn memcpy(dest: *mut u8, src: *const u8, num: usize) -> *mut u8 {
    let mut dest_i = dest;
    let mut src_i = src;
    for i in 0..num {
        unsafe {
            *dest_i = *src_i;
            src_i = core::mem::transmute(core::mem::transmute::<_, usize>(src_i) + 1);
            dest_i = core::mem::transmute(core::mem::transmute::<_, usize>(dest_i) + 1);
        }
    }

    dest
}

#[cfg(target_arch = "avr")]
#[no_mangle]
pub extern "C" fn memset(dest: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut dest_i = dest;
    for i in 0..n {
        unsafe { *dest_i = c as u8 };
    }

    dest
}
