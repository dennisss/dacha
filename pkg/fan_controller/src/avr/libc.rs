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
