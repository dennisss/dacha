
use crate::avr::pins::PB0;


#[cfg(target_arch = "avr")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    unsafe { crate::avr::disable_interrupts(); };

    crate::usart::USART1::send_blocking(b"E:\n");
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        crate::usart::USART1::send_blocking(s.as_bytes());
        crate::usart::USART1::send_blocking(b"\n");
    }

    abort();
}

#[cfg(target_arch = "avr")]
#[no_mangle]
pub extern "C" fn abort() -> ! {
    // TODO: Shut off the PLL
    // TODO: Disable all clocks, interrupts, and go to sleep.
    // (otherwise we will still be sending signals to fans, etc.)

    // crate::usart::USART1::send_blocking(b"ABORT\n");

    unsafe {
        crate::avr::disable_interrupts();
    }

    loop {
        // TODO: Need a customizable error pin. 
        PB0::write(false);
        for _i in 0..100000 {
            unsafe {
                llvm_asm!("nop");
            }
        }
        PB0::write(true);
        for _i in 0..100000 {
            unsafe {
                llvm_asm!("nop");
            }
        }
    }
}

#[inline(never)]
pub fn avr_assert_impl(v: bool) {
    if !v {
        panic!();
    }
}

#[macro_export]
macro_rules! avr_assert {
    ($e: expr) => ({
        if !($e) {
            unsafe { crate::usart::USART1::send_blocking(b"AS\n") };
            // $crate::avr::assert::avr_assert_impl(false);
            panic!();
        }

        // 
    });
}

#[macro_export]
macro_rules! avr_assert_eq {
    ($left: expr, $right: expr) => ({
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    unsafe { crate::usart::USART1::send_blocking(b"EQ\n") };
                    panic!();
                    // $crate::avr::assert::avr_assert_impl(false)
                }
            }
        }
    });
}

#[macro_export]
macro_rules! avr_assert_ne {
    ($left: expr, $right: expr) => ({
        match (&$left, &$right) {
            (left_val, right_val) => {
                if (*left_val == *right_val) {
                    unsafe { crate::usart::USART1::send_blocking(b"NE\n") };

                    panic!();
                    // $crate::avr::assert::avr_assert_impl(false);
                }
            }
        }
    });
}
