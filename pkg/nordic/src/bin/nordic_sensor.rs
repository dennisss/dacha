#![feature(type_alias_impl_trait, impl_trait_in_assoc_type)]
#![no_std]
#![no_main]


#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate macros;

#[macro_use]
extern crate executor;
extern crate peripherals;
#[macro_use]
extern crate common;
#[macro_use]
extern crate nordic;
#[macro_use]
extern crate logging;

use core::arch::asm;

use executor::CriticalSection;
use nordic::protocol::sensor_usb_thread_fn;
use nordic::radio_socket::*;
use nordic::radio::*;
use nordic::rtc::RTC;
use nordic::uarte::UARTE;
use nordic::ecb::ECB;
use nordic::gpio::GPIO;
use nordic::controller::PeripheralsController;
use nordic::usb::controller::USBDeviceController;
use nordic::rng::*;
use nordic_wire::usb_descriptors::*;
use executor::singleton::Singleton;
use nordic::params::AppParamsStorage;
use protobuf::Message;
use nordic::sensor::config_store::SensorConfigStore;
use nordic::sensor::controller::SensorController;
use nordic_proto::nordic::SensorPacket;

// TODO: Need to get rid of most of the internal buffers in this since we effectively
// are only using this for config storage.
static RADIO_SOCKET: RadioSocket = RadioSocket::new();

static PARAMS_STORAGE: Singleton<AppParamsStorage> = Singleton::uninit();

static SENSOR_CONFIG_STORAGE: Singleton<SensorConfigStore> = Singleton::uninit();

static SENSOR_CONTROLLER: Singleton<SensorController> = Singleton::uninit();


define_thread!(Button, button_thread_fn);
async fn button_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = RTC::new(peripherals.rtc0);

    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    let mut prng = {
        let mut rng = Rng::new(peripherals.rng);
        let mut seed = [0u32; 4];
        rng.generate(&mut seed).await;
        Xoshiro128PlusPlus::new(seed)
    };

    let params_storage = {
        PARAMS_STORAGE
            .set(AppParamsStorage::create(peripherals.nvmc).unwrap())
            .await
    };

    let sensor_config_store = {
        SENSOR_CONFIG_STORAGE
            .set(SensorConfigStore::create(params_storage).await.unwrap())
            .await
    };

    RADIO_SOCKET
        .configure_storage(params_storage)
        .await
        .unwrap();

    let mut radio_controller = RadioController::new(
        &RADIO_SOCKET,
        Radio::new(peripherals.radio),
        ECB::new(peripherals.ecb),
    );
    // NOTE: We don't start this since we will manually trigger all TX/RX operations to
    // conserve power.
    // TODO: Give this unique interrupts
    // RadioControllerThread::start(radio_controller);

    ButtonUSBThread::start(
        SENSOR_USB_DESCRIPTORS,
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        &RADIO_SOCKET,
        sensor_config_store,
        timer.clone(),
    );

    let sensor_config = sensor_config_store.get_config().await.unwrap();

    let sensor_controller = {
        SENSOR_CONTROLLER
            .set(SensorController::new(
                sensor_config,
                timer.clone(),
                radio_controller,
                prng,
                gpio
            ))
            .await
    };

    sensor_controller.start();

}

define_thread!(
    ButtonUSBThread,
    sensor_usb_thread_fn,
    descriptors: SensorUSBDescriptors,
    usb: USBDeviceController,
    radio_socket: &'static RadioSocket,
    sensor_config_store: &'static SensorConfigStore,
    rtc: RTC
);


// TODO: Reduce
const RAM_SIZE: u32 = 32 * 1024;

entry!(main);
fn main() -> () {
    // TODO: Disable interrupts first.
    reset_stack(nordic::ram::RAM_START_ADDRESS + RAM_SIZE, main_inner);
}

extern "C" fn main_inner() -> ! {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    let cs = CriticalSection::new();

    let mut peripherals = peripherals::raw::Peripherals::new();

    // TODO: We can probably save more power by shifting forward the start address rather then reducing the end
    // address (to disable more RAM controllers).
    nordic::ram::configure_retained_ram(0, RAM_SIZE, &mut peripherals.power);

    // This seems to increase power usage.
    peripherals.power.dcdcen.write_enabled();

    peripherals.power.tasks_lowpwr.write_trigger();


    // nordic::clock::reference_hfclk();
    nordic::clock::init_low_freq_clk(
        nordic::clock::LowFrequencyClockSource::RCOscillator,
        &mut peripherals.clock,
    );

    // TODO: Do this consistently outside of the bootloader.
    peripherals.nvmc.icachecnf.write_with(|v| v.set_cacheen_with(|v| v.set_enabled()));

    // nordic::enable_fpu();

    Button::start();

    // Enable interrupts.
    drop(cs);

    loop {
        unsafe { asm!("wfi") };
    }
}

/// Sets the stack pointer to new_sp and then jumps to 'f'.
fn reset_stack(new_sp: u32, f: extern "C" fn() -> !) -> ! {
    unsafe {
        asm!(
            "mov sp, {sp}",
            "bx {ep}",
            sp = in(reg) new_sp,
            ep = in(reg) f,
            options(noreturn)
        );
    }
}