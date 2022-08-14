use executor::channel::Channel;

use crate::gpio::*;
use crate::radio_socket::RadioController;
use crate::timer::{Timer, TimerInstant};

const LED_ON_OFF_TIME_MS: usize = 100;
const ACTIVITY_TIMEOUT_MS: usize = 2 * LED_ON_OFF_TIME_MS;

static TX_EVENT: Channel<()> = Channel::new();
static RX_EVENT: Channel<()> = Channel::new();

/// Configured LEDs to be turned on to indicate TX/RX of packets in the radio.
///
/// - LEDs are active-low
/// - The blink rate of the LEDs is constant and the number of blinks does not
///   indicate the number of packets sent/received. Instead the duration of the
///   blinking indicates that some activity has occured recently.
/// - This can only be configured once in the entire program as we only define
///   one thread internally.
pub fn setup_radio_activity_leds(
    tx_pin: GPIOPin,
    rx_pin: GPIOPin,
    timer: Timer,
    radio_controller: &mut RadioController,
) {
    radio_controller.set_tx_event(&TX_EVENT);
    radio_controller.set_rx_event(&RX_EVENT);
    internal::RadioActivityLEDThread::start(tx_pin, rx_pin, timer)
}

mod internal {
    use super::*;

    define_thread!(
        RadioActivityLEDThread,
        radio_activity_thread_fn,
        tx_pin: GPIOPin,
        rx_pin: GPIOPin,
        timer: Timer
    );
    async fn radio_activity_thread_fn(tx_pin: GPIOPin, rx_pin: GPIOPin, timer: Timer) {
        race!(
            run_single_led(tx_pin, &TX_EVENT, timer.clone()),
            run_single_led(rx_pin, &RX_EVENT, timer.clone()),
        )
        .await;
    }
}

async fn run_single_led(mut led_pin: GPIOPin, event: &'static Channel<()>, mut timer: Timer) {
    led_pin
        .set_direction(PinDirection::Output)
        .write(PinLevel::High);

    let mut last_time = None;

    loop {
        let now = timer.now();
        if event.try_recv().await.is_some() {
            last_time = Some(now);
        } else if let Some(t) = &last_time {
            if now.millis_since(t) > ACTIVITY_TIMEOUT_MS {
                last_time = None;
            }
        }

        if last_time.is_some() {
            if (now.millis_since(&TimerInstant::zero()) / LED_ON_OFF_TIME_MS) % 2 == 0 {
                led_pin.write(PinLevel::Low);
            } else {
                led_pin.write(PinLevel::High);
            }
        } else {
            led_pin.write(PinLevel::High);
        }

        timer.wait_ms(50).await;
    }
}
