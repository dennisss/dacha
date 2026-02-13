// This is the singleton thread that is spawned when a 'measure_mcu_temperature'
// request is received.

use peripherals_proto::peripherals::PeripheralResponse;

use crate::controller::peripherals_controller::PeripheralsController;
use crate::temp::Temp;

define_thread!(
    TemperaturePeripheralThread,
    temperature_peripheral_thread_fn,
    controller: &'static PeripheralsController,
    request_sequence: u32,
    temp: Temp
);

async fn temperature_peripheral_thread_fn(
    controller: &'static PeripheralsController,
    request_sequence: u32,
    mut temp: Temp,
) {
    executor::interrupts::yield_now().await;

    let value = temp.measure().await;

    lock!(state <= controller.state.lock(), {
        state.temp = Some(temp);

        let mut res = PeripheralResponse::default();
        res.set_request_sequence(request_sequence);
        // TODO: Move the conversion to the 'Temp' struct.
        res.set_float_val((value as f32) * 0.25);

        controller.write_response(&mut state, &res);
    });
}
