/*
Doing a fan tachometer reading:

- Pull up the pin
- Count the time between '1 -> 0' pulses.
- For 120mm fan: [15ms, 66ms] (for very fast 3d printer fans, maybe down to 3ms)
- Simple solution is to setup an interrupt.
    - Wait for the interrupt on the pin to go low
    - Then wait again for the second pulse.
*/

use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
};
use peripherals::raw::gpiote::GPIOTE;

use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::{PeripheralEntry, GPIOEntry};
use crate::gpio::GPIOPin;
use crate::gpiote::GPIOInterruptPolarity;
use crate::rtc::RTC;

/// Max amount of time in milliseconds we will wait before assuming the fan is
/// off and thus not reporting any speed.
///
/// This needs to be at least 2x the max period we want to measure (since we
/// require measuring 2 falling edges).
const TIMEOUT_MILLIS: u32 = 150;

define_thread!(
    TachometerPeripheralThread,
    tachometer_worker_thread,
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    entry: GPIOEntry
);

async fn tachometer_worker_thread(
    controller: &'static PeripheralsController,
    peripheral_index: usize,
    request_sequence: u32,
    entry: GPIOEntry,
) {
    // TODO: Handle failure of this on unwrap.
    let mut int = lock!(state <= controller.state.lock().await.unwrap(), {
        state.gpiote.new_interrupt_channel(&entry.pin, GPIOInterruptPolarity::FallingEdge)
    }).unwrap();

    let mut clock1 = controller.clock.clone();
    let timeout = async {
        clock1.wait_ms(TIMEOUT_MILLIS).await;
        None
    };

    let mut clock2 = controller.clock.clone();

    // Clear any initial events.
    int.pending_events();

    let collector = async {
        while !int.wait_for_interrupts().await {
            continue;
        }
        let t1 = clock2.now();

        while !int.wait_for_interrupts().await {
            continue;
        }
        let t2 = clock2.now();

        // TODO: Need to disable the interrupt somewhere
        // gpio_interrupts.reset();

        Some(t2.micros_since(&t1))
    };

    let result = race!(collector, timeout).await;

    lock!(state <= controller.state.lock().await.unwrap(), {
        state.entries[peripheral_index] = PeripheralEntry::GPIO(entry);

        let mut res = PeripheralResponse::default();
        res.set_request_sequence(request_sequence);

        if let Some(val) = result {
            res.set_uint_val(val as u32);
        } else {
            res.set_error_code(PeripheralResponse_ErrorCode::TIMEOUT);
        };

        controller.write_response(&mut state, &res);
    });
}
