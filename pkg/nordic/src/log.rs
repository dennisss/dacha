use executor::mutex::Mutex;

use crate::uarte::UARTE;

static LOGGER_INSTANCE: Mutex<Option<UARTE>> = Mutex::new(None);

pub async fn setup(uarte: UARTE) {
    let mut inst = LOGGER_INSTANCE.lock().await;
    *inst = Some(uarte);
}

pub async fn log_write(data: &[u8]) {
    let mut inst = LOGGER_INSTANCE.lock().await;
    if let Some(inst) = inst.as_mut() {
        inst.write(data).await;
    }
}

#[macro_export]
macro_rules! log {
    ($s:expr) => {
        $crate::log::log_write($s).await
    };
}

pub struct NumberSlice {
    buf: [u8; 10],
    len: usize,
}

impl AsRef<[u8]> for NumberSlice {
    fn as_ref(&self) -> &[u8] {
        &self.buf[(self.buf.len() - self.len)..]
    }
}

pub fn num_to_slice(mut num: u32) -> NumberSlice {
    // A u32 has a maximum length of 10 base-10 digits
    let mut buf: [u8; 10] = [0; 10];
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

    NumberSlice {
        buf,
        len: num_digits,
    }
}
