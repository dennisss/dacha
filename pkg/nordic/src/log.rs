use executor::mutex::Mutex;

use crate::uarte::UARTE;

static LOGGER_INSTANCE: Mutex<Option<UARTE>> = Mutex::new(None);

pub async fn setup(uarte: UARTE) {
    let mut inst = LOGGER_INSTANCE.lock().await;
    *inst = Some(uarte);
}

pub async fn log_write(data: &[u8]) {
    let mut inst = LOGGER_INSTANCE.lock().await;
    inst.as_mut().unwrap().write(data).await;
}

#[macro_export]
macro_rules! log {
    ($s:expr) => {
        $crate::log::log_write($s).await
    };
}
