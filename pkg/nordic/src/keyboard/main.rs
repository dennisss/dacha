use executor::mutex::Mutex;
use executor::singleton::Singleton;
use peripherals::raw::{PinDirection, PinLevel};
use usb::hid::StandardKeyboardInputReport;

use crate::config_storage::NetworkConfigStorage;
use crate::ecb::ECB;
use crate::gpio::Resistor;
use crate::gpio::GPIO;
use crate::keyboard::key_scanner::KeyScanner;
use crate::keyboard::shift_register::ShiftRegister;
use crate::keyboard::state::KeyboardState;
use crate::keyboard::state::KeyboardUSBProtocol;
use crate::keyboard::usb_handler::KeyboardUSBHandler;
use crate::keyboard::usb_handler::*;
use crate::params::ParamsStorage;
use crate::radio::Radio;
use crate::radio_socket::{RadioController, RadioControllerThread, RadioSocket};
use crate::timer::Timer;
use crate::usb::controller::USBDeviceController;
use crate::usb::send_buffer::USBDeviceSendBuffer;

static PARAMS_STORAGE: Singleton<ParamsStorage> = Singleton::uninit();

static RADIO_SOCKET: RadioSocket = RadioSocket::new();

static REPORT_SEND_BUFFER: USBDeviceSendBuffer = USBDeviceSendBuffer::new();

static STATE: Mutex<KeyboardState> = Mutex::new(KeyboardState {
    // 500ms as recommended in section 7.2.4 of the HID v1.11 spec.
    idle_timeout: 500,

    protocol: KeyboardUSBProtocol::Report,
});

define_thread!(
    KeyboardUSBThread,
    keyboard_usb_thread_fn,
    usb_controller: USBDeviceController,
    handler: KeyboardUSBHandler
);
async fn keyboard_usb_thread_fn(
    mut usb_controller: USBDeviceController,
    handler: KeyboardUSBHandler,
) {
    usb_controller.run(handler).await;
}

define_thread!(Main, main_thread_fn);
async fn main_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { crate::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);

    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    let mut led_enable = gpio.pin(pins.P0_02);
    let led_serial = pins.P0_04;

    // TODO: This doesn't work if the LEDs are already illuminated?
    led_enable
        .set_direction(PinDirection::Output)
        .write(PinLevel::Low);

    let params_storage = {
        PARAMS_STORAGE
            .set(ParamsStorage::create(peripherals.nvmc).unwrap())
            .await
    };

    RADIO_SOCKET
        .configure_storage(params_storage)
        .await
        .unwrap();

    // TODO: Ideally we would support turning this off when we are in USB mode.
    let mut radio_controller = RadioController::new(
        &RADIO_SOCKET,
        Radio::new(peripherals.radio),
        ECB::new(peripherals.ecb),
    );
    RadioControllerThread::start(radio_controller);

    let mut usb_controller = USBDeviceController::new(peripherals.usbd, peripherals.power);
    usb_controller.set_send_buffer(&REPORT_SEND_BUFFER);

    KeyboardUSBThread::start(
        usb_controller,
        KeyboardUSBHandler::new(&STATE, &RADIO_SOCKET, timer.clone()),
    );

    log!("Started up!");

    /*
    let mut key_anode = gpio.pin(pins.P0_30);
    key_anode
        .set_direction(PinDirection::Output)
        .write(PinLevel::High);

    let mut key_cathode = gpio.pin(pins.P0_31);
    key_cathode
        .set_direction(PinDirection::Input)
        .set_resistor(Resistor::PullDown);

    let mut blink_pin = {
        gpio.pin(pins.P0_15)
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);

        gpio.pin(pins.P0_14)
    };
    */

    /*
    let mut led_spi = SPIHost::new(
        peripherals.spim0,
        8_000_000,
        led_serial,
        pins.P0_28,           // Charge state
        pins.P0_29,           // Battery current count
        gpio.pin(pins.P0_30), // Batter scaled voltage
        SPIMode::Mode0,
    );

    let colors = [
        expand_color(0x00110000),
        expand_color(0x00001100),
        expand_color(0x00000011),
    ];

    let black = expand_color(0);
    for i in 0..103 {
        led_spi.transfer(&black[..], &mut []).await;
    }

    for k in 0..103 {
        for i in 0..k {
            led_spi
                .transfer(&colors[i % colors.len()][..], &mut [])
                .await;
        }

        timer.wait_ms(2000).await;
    }
    */

    log!("Run!");

    let x_register = ShiftRegister::new(
        gpio.pin(pins.P0_31),
        gpio.pin(pins.P0_01),
        gpio.pin(pins.P0_00),
    );

    let mut y_inputs = [
        gpio.pin(pins.P0_15),
        gpio.pin(pins.P0_17),
        gpio.pin(pins.P0_20),
        gpio.pin(pins.P0_09),
        gpio.pin(pins.P0_10),
        gpio.pin(pins.P0_03),
    ];

    for pin in &mut y_inputs {
        pin.set_direction(PinDirection::Input)
            .set_resistor(Resistor::PullDown);
    }

    let mut key_scanner = KeyScanner {
        x_register,
        y_inputs,
    };

    let mut last_report = StandardKeyboardInputReport::default();

    // Continously poll for key presses.
    loop {
        timer.wait_ms(10).await;

        let report = key_scanner.scan().await;
        if report.as_ref() != last_report.as_ref() {
            REPORT_SEND_BUFFER.write(report.as_ref()).await;
        }

        last_report = report;
    }
}
