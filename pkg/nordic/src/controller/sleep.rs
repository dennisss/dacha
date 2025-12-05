use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};
use peripherals::raw::gpiote::GPIOTE;

use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;
use crate::gpio::GPIOPin;
use crate::gpiote::GPIOInterruptPolarity;
use crate::rtc::RTC;

define_thread!(
    SleepPeripheralThread,
    sleep_peripheral_thread_fn,
    controller: &'static PeripheralsController,
    request_sequence: u32
);

async fn sleep_peripheral_thread_fn(
    controller: &'static PeripheralsController,
    request_sequence: u32,
) {
    let mut clock = controller.clock.clone();
    clock.wait_ms(1000).await;

    lock!(state <= controller.state.lock().await.unwrap(), {
        let mut res = PeripheralResponse::default();
        res.set_request_sequence(request_sequence);
        controller.write_response(&mut state, &res);
    });
}
