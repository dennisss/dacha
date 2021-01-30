macro_rules! debug {
    ($a:expr) => {{
        let tx = $crate::avr::usart::USART1Transmission::start().await;
        ($a).debug_write(&mut tx);
        drop(tx);
    }};
}

#[inline(never)]
pub fn uart_send_number_sync(num: u8) {
    num_to_slice(num, |data| uart_send_sync(data));
}

fn num_to_slice<F: FnOnce(&[u8])>(mut num: u8, f: F) {
    // A u32 has a maximum length of 10 base-10 digits
    let mut buf: [u8; 3] = [0; 3];
    let mut num_digits = 0;
    while num > 0 {
        // TODO: perform this as one operation?
        let r = (num % 10) as u8;
        num /= 10;

        num_digits += 1;

        buf[buf.len() - num_digits] = ('0' as u8) + r;
    }

    if num_digits == 0 {
        num_digits = 1;
        buf[buf.len() - 1] = '0' as u8;
    }

    f(&buf[(buf.len() - num_digits)..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_num_to_slice() {
        fn run_test(num: u8, expected: &'static [u8]) {
            let mut called = false;
            num_to_slice(num, |v| {
                called = true;
                assert_eq!(v, expected);
            });
            assert!(called);
        }

        run_test(0, b"0");
        run_test(1, b"1");
        run_test(2, b"2");
        run_test(3, b"3");
        run_test(50, b"50");
        run_test(100, b"100");
        run_test(202, b"202");
        run_test(255, b"255");
    }
}
