// This is the singleton thread that is continously running and waits for resets
// and peripheral that has timed out due to lack of activity.

use peripherals_proto::peripherals::PeripheralResponse;

use crate::controller::peripherals_controller::PeripheralsController;
use crate::controller::PeripheralEntry;
use crate::temp::Temp;

define_thread!(
    TimeoutPeripheralThread,
    timeout_peripheral_thread_fn,
    controller: &'static PeripheralsController
);

async fn timeout_peripheral_thread_fn(controller: &'static PeripheralsController) {
    let mut rtc = controller.clock.clone();
    loop {
        rtc.wait_ms(1000).await;

        lock!(state <= controller.state.lock().await.unwrap(), {
            let now = rtc.now();

            let state = &mut *state;

            for entry in &mut state.entries {
                match entry {
                    PeripheralEntry::PWM {
                        index,
                        channel,
                        inverted,
                        default_value,
                        last_active,
                        timeout_millis,
                    } => {
                        let last_active_value = match last_active.clone() {
                            Some(v) => v,
                            None => continue,
                        };

                        let timeout_millis_value = match timeout_millis.clone() {
                            Some(v) => v,
                            None => continue,
                        };

                        if now.millis_since(&last_active_value) as u32 > timeout_millis_value {
                            *last_active = None;
                            state.pwms[*index].set_value(*channel, *default_value, *inverted);
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}
