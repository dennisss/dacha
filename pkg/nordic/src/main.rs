#![feature(
    lang_items,
    type_alias_impl_trait,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types,
    impl_trait_in_assoc_type
)]
#![no_std]
#![no_main]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate executor;
extern crate peripherals;
#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate nordic;

extern crate math;

/*
General workflow:
- Start up
- Read from EEPROM to see if there are initial values of the
    - Increment counter by 100
- Over USB we can do SetNetworkConfig to reconfigure it and set the counter to 0
    - The host should use GetNetworkConfig to not accidentally reset a counter if the keys han't changed.
    - NOTE: Before this happens, we can't run the RadioSocket thread with a bad config.
- Every 100 packets, save the counter to EEPROM before sending it.
- Eventually, every 10 seconds, save to EEPROM the last packet counter received from each remote link

*/

use core::arch::asm;

use common::register::{RegisterRead, RegisterWrite};
use executor::singleton::Singleton;
use logging::log;
use peripherals::eeprom::EEPROM;
use peripherals::raw::rtc0::RTC0;
use peripherals::raw::PinLevel;
use peripherals::storage::BlockStorage;

use nordic::config_storage::NetworkConfigStorage;
use nordic::ecb::ECB;
use nordic::eeprom::Microchip24XX256;
use nordic::gpio::*;
// use nordic::protocol::ProtocolUSBThread;
use nordic::radio::Radio;
use nordic::radio_socket::{RadioController, RadioControllerThread, RadioSocket};
use nordic::rng::Rng;
use nordic::spi::*;
use nordic::temp::Temp;
use nordic::timer::Timer;
use nordic::tmc2130::TMC2130;
use nordic::twim::TWIM;
use nordic::uarte::UARTE;
use nordic::usb::controller::USBDeviceController;
use nordic::usb::default_handler::USBDeviceDefaultHandler;

/*
Allocator design:
- Current horizon pointer (initialized at end of static RAM)
    - Increment horizon pointer when we want to allocate more memory
    -> do need to

*/

/*
Dev kit LEDs
    P0.13
    P0.14
    P0.15
    P0.16

    active low

Dongle LEDS
    Regular:
        P0.06
    RGB
        P0.08
        P1.09
        P0.12


    active low
*/

/*
TMC2130 SilentStepStick
- Schematic: file:///home/dennis/Downloads/SilentStepStick-TMC2130_v20.pdf

- Connections:
    - Keep solder bridge open

    - EN : The power stage becomes switched off (all motor outputs floating) when this pin becomes driven to a high level.
    - SDI/SCK/CS/SDO: Use for SPI
    - DCO: ?
    - STEP:
    - DIR: P0_00
    - VM : 12V
    - GND: GND
    - M*
    - VIO: 5V
    - GND: GND

- Communication between an SPI master and the TMC2130 slave always consists of sending one 40-bit
command word and receiving one 40-bit status word

- TODO: Set: internal_Rsense

Example from Data sheet
    SPI send: 0xEC000100C3; // CHOPCONF: TOFF=3, HSTRT=4, HEND=1, TBL=2, CHM=0 (spreadCycle)
    SPI send: 0x9000061F0A; // IHOLD_IRUN: IHOLD=10, IRUN=31 (max. current), IHOLDDELAY=6
    SPI send: 0x910000000A; // TPOWERDOWN=10: Delay before power down in stand still
    SPI send: 0x8000000004; // EN_PWM_MODE=1 enables stealthChop (with default PWM_CONF)
    SPI send: 0x93000001F4; // TPWM_THRS=500 yields a switching velocity about 35000 = ca. 30RPM
    SPI send: 0xF0000401C8; // PWM_CONF: AUTO=1, 2/1024 Fclk, Switch amplitude limit=200, Grad=1


- Read IOIN (0x04 : top byte should be 0x11 (version)), bit 6 should be 1
- Write GCONF: 0x00 to 0
- Write IHOLD_IRUN: 0x10 to IHOLD=0, IRUN= XX,  IHOLDDELAY=6
- WRITE TPOWER DOWN: 0x11 to 10

*/

const USING_DEV_KIT: bool = true;

static RADIO_SOCKET: RadioSocket = RadioSocket::new();
static BLOCK_STORAGE: Singleton<BlockStorage<Microchip24XX256>> = Singleton::uninit();

/*
Given a LinearMotion in step units, execute it.
- Need to know start position, current position and start time.
- Use time_to_travel to determine when the next step should be performed
- Execute this tick

For 100ms/s, I'd need to support 10K steps per second.
*/

/*
[
    LinearMotion {
        start_position: 0.0
        start_velocity: 0.0
        end_position: 10240.0
        end_velocity: 3200.0
        acceleration: 500.0
        duration: 6.4,
    },
    LinearMotion {
        start_position: 10240.0
        start_velocity: 3200.0
        end_position: 53760.0
        end_velocity: 3200.0
        acceleration: 0.0
        duration: 13.6,
    },
    LinearMotion {
        start_position: 53760.0
        start_velocity: 3200.0
        end_position: 64000.0
        end_velocity: 0.0
        acceleration: -500.0
        duration: 6.4,
    },
]

*/

/*
use cnc::linear_motion::LinearMotion;
use math::matrix::Vector3f;

async fn run_cnc(mut timer: Timer, mut step_pin: GPIOPin) {
    let motions = &[
        LinearMotion {
            start_position: Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            start_velocity: Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            end_position: Vector3f::from_slice(&[10240.0, 0.0, 0.0]),
            end_velocity: Vector3f::from_slice(&[3200.0, 0.0, 0.0]),
            acceleration: Vector3f::from_slice(&[500.0, 0.0, 0.0]),
            duration: 6.4,
        },
        LinearMotion {
            start_position: Vector3f::from_slice(&[10240.0, 0.0, 0.0]),
            start_velocity: Vector3f::from_slice(&[3200.0, 0.0, 0.0]),
            end_position: Vector3f::from_slice(&[53760.0, 0.0, 0.0]),
            end_velocity: Vector3f::from_slice(&[3200.0, 0.0, 0.0]),
            acceleration: Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            duration: 13.6,
        },
        LinearMotion {
            start_position: Vector3f::from_slice(&[53760.0, 0.0, 0.0]),
            start_velocity: Vector3f::from_slice(&[3200.0, 0.0, 0.0]),
            end_position: Vector3f::from_slice(&[64000.0, 0.0, 0.0]),
            end_velocity: Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            acceleration: Vector3f::from_slice(&[-500.0, 0.0, 0.0]),
            duration: 6.4,
        },
    ];

    let mut current_position: i32 = 0;

    for motion in motions {
        // NOTE: Assume current_position == motion.start_position[0]

        // TODO: Eventually use the absolute start time.
        let mut current_time = timer.now();

        let start_position = motion.start_position[0] as i32;

        assert_no_debug!(start_position == current_position);

        let end_position = motion.end_position[0] as i32;

        let step_dir = 1; // Forwards

        while current_position != end_position {
            let next_position = current_position + step_dir;

            let end_time = current_time.add_seconds(cnc::kinematics::time_to_travel(
                (next_position - start_position) as f32,
                motion.start_velocity[0],
                motion.acceleration[0],
            ));

            step_pin.write(PinLevel::High);
            for i in 0..10 {
                unsafe { asm!("nop") };
            }
            step_pin.write(PinLevel::Low);

            timer.wait_until(end_time).await;

            current_position = next_position;
        }

        // log!("Y");
    }

    log!("Z");
}
*/

define_thread!(Blinker, blinker_thread_fn);
async fn blinker_thread_fn() {
    let mut peripherals = peripherals::raw::Peripherals::new();
    let mut pins = unsafe { nordic::pins::PeripheralPins::new() };

    let mut timer = Timer::new(peripherals.rtc0);

    let temp = Temp::new(peripherals.temp);

    let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);

    {
        let mut serial = UARTE::new(peripherals.uarte0, pins.P0_30, pins.P0_31, 115200);
        // TODO:
        // log::setup(serial).await;
    }

    log!("Started up!");

    loop {
        log!("Hi!");

        timer.wait_ms(1500).await;
    }

    return;

    /*
    DIR: P0_11
    STEP: P0_13
    SDO: P0_05
    CS: P0_06
    SCK: P0_07
    SDI: P0_08
    EN: P0_09 : Drive me high to turn off.
    */

    {
        let mut gpio_int = GPIOInterrupts::new(peripherals.gpiote);

        let mut dir_pin = gpio.pin(pins.P0_11);
        let mut step_pin = gpio.pin(pins.P0_13);
        let mut sdo_pin = pins.P0_05;
        let mut cs_pin = gpio.pin(pins.P0_06);
        let mut sck_pin = pins.P0_07;
        let mut sdi_pin = pins.P0_08;
        let mut en_pin = gpio.pin(pins.P0_09);

        let mut diag1 = gpio_int.setup_interrupt(pins.P0_27, GPIOInterruptPolarity::RisingEdge);

        dir_pin
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);
        step_pin
            .set_direction(PinDirection::Output)
            .write(PinLevel::High);

        en_pin
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);

        let mut spi = SPIHost::new(
            peripherals.spim0,
            250_000,
            sdi_pin,
            sdo_pin,
            sck_pin,
            cs_pin,
            SPIMode::Mode3,
        );

        log!("Ready...!");

        let mut tmc = TMC2130::new(spi);

        // Read and verify IOIN
        {
            let num = tmc.read_register(0x04).await.to_be_bytes();

            for i in 0..4 {
                log!(num[i] as u32, ", ");
            }
            log!("\n");

            if num[0] != 0x11 {
                return;
            }
        }

        log!("Config");

        /*

        internal f_clk = ~13MHz

        (1 / speed) * (1 / (6.25*256))

        1.25e-5 / (1 / 13000000)

        at 6.25*256 steps/mm,
            1mm/s is 0.000625 s/step  => 8125.0 TSTEP
            10mm/s is 0.00625 s/step  => 812.5  TSTEP
            50mm/s is 1.25e-5 s/step  => 162.5  TSTEP
            100mm/s is 6.25e-6        => 81.25  TSTEP


        Irms = (Vref * 1.77A) / 2.5V = Vref * 0.71


        I_scale_analog = true


        Write default values as recommended in the TMC2130 data sheet,

        SPI send: 0xEC000100C3; // CHOPCONF: TOFF=3, HSTRT=4, HEND=1, TBL=2, CHM=0 (spreadCycle)
        SPI send: 0x9000061F0A; // IHOLD_IRUN:
        SPI send: 0x910000000A; // TPOWERDOWN=10: Delay before power down in stand still
        SPI send: 0x8000000004; // EN_PWM_MODE=1 enables stealthChop (with default PWM_CONF)
        SPI send: 0x93000001F4; // TPWM_THRS=500 yields a switching velocity about 35000 = ca. 30RPM
        SPI send: 0xF0000401C8; // PWM_CONF: AUTO=1, 2/1024 Fclk, Switch amplitude limit=200, Grad=1
        */

        // CHOPCONF
        // Datasheet Recommended: TOFF=3, HSTRT=4, HEND=1, TBL=2, CHM=0 (spreadCycle)
        // MRES = 4 (1/16 microstepping)
        // intpol=1
        tmc.write_register(0x6C, 0x000100C3 | (4 << 24) | (1 << 28))
            .await;

        // signed twos complement 7-bit.
        let sgt: i8 = 6;

        // COOLCONF
        // sfilt=1
        // semin=5
        // semax=2
        // sedn=1
        tmc.write_register(
            0x6D,
            (1 << 24) | ((sgt as u32) << 16), /* | (5 << 0) | (2 << 8) | (1 << 13) */
        )
        .await;

        // IHOLD_IRUN
        // Datasheet Recommended: IHOLD=10, IRUN=31 (max. current), IHOLDDELAY=6
        tmc.write_register(0x10, 0x00061F0A).await;

        tmc.write_register(0x11, 0x0000000A).await;

        // TPWMTHRS
        tmc.write_register(0x13, 0).await;

        // TCOOLTHRS
        tmc.write_register(0x14, /* 9000 */ 0xFFFFF).await;

        // THIGH
        tmc.write_register(0x15, 0).await;

        // GCONF
        // Datasheet Recommended:  EN_PWM_MODE=1 enables stealthChop (with default
        // PWM_CONF) I_scale_analog=1
        // diag1_pushpull=1 (Active high)
        // diag1_stall=1 (Enable stall output): MUST set TCOOLTHRS before using this.
        tmc.write_register(0x00, 0x00000004 | (1 << 0) | (1 << 13) | (1 << 8))
            .await;

        tmc.write_register(0x70, 0x000401C8).await;

        /*
                TODO: Coolstep

                Tuning StallGuard2
                - Enable sfilt=1 (samples every 4 full steps)

            -
        (configure properly, also set
        TCOOLTHRS
                */

        log!("Run");

        // let mut blink = gpio.pin(pins.P0_14);
        // blink.set_direction(PinDirection::Output);

        // return run_cnc(timer, step_pin).await;

        let mut value = false;
        let mut count = 0;

        loop {
            let e = gpio_int.pending_events();
            if e.contains(diag1) {
                log!("STALE");
                break;
            }

            // blink.write(PinLevel::Low);
            step_pin.write(if value { PinLevel::Low } else { PinLevel::High });
            value = !value;

            timer.wait_micros(100).await;
            // for i in 0..1000 {
            //     unsafe { asm!("nop") };
            // }

            count += 1;

            if count % 1000 == 0 {
                let a = tmc.read_register(0x12).await;
                let v = tmc.read_register(0x6F).await;

                log!(a, " : ", v & 0x3FF);
            }
        }
    }

    /*
    TODO: Must implement alternatePeripheral in CMSIS SVD conversion.
    - Any peripheral that re-use the same memory block require special attention.
    */

    // WP 3, SCL 4, SDA 28

    {
        // TODO: Set these pins as Input with S0D1 drive strength,

        // addr = 80

        /*
        for i in 0..127 {
            log!(nordic::log::num_to_slice(i as u32).as_ref());
            log!(b"\n");

            match twim.read(i, &mut []).await {
                Ok(_) => {
                    // log!(b"GOOD: ");
                }
                Err(_) => {}
            }
        }
        */

        /*
        if let Err(e) = eeprom.write(0, b"ABCDE").await {
            log!("WRITE FAIL");
        }

        let mut buf = [0u8; 5];
        if let Err(e) = eeprom.read(0, &mut buf).await {
            log!("READ FAIL");
        }

        // TODO: Also verify read and write from arbitrary non-zero locations.

        log!("READ:");
        log!(&buf);
        log!(b"\n");
        */
    }

    // Helper::start(timer.clone());

    //

    // TODO: Which Send/Sync requirements are needed of these arguments?
    // Echo::start(
    //     peripherals.uarte0,
    //     timer.clone(),
    //     temp,
    //     Rng::new(peripherals.rng),
    // );

    /*
    let radio_socket = &RADIO_SOCKET;

    let radio_controller = RadioController::new(
        radio_socket,
        Radio::new(peripherals.radio),
        ECB::new(peripherals.ecb),
    );

    let block_storage = {
        let mut twim = TWIM::new(peripherals.twim0, pins.P0_04, pins.P0_28, 100_000);
        let mut eeprom = Microchip24XX256::new(twim, 0b1010000, gpio.pin(pins.P0_03));
        BLOCK_STORAGE.set(BlockStorage::new(eeprom)).await
    };

    RADIO_SOCKET
        .configure_storage(NetworkConfigStorage::open(block_storage).await.unwrap())
        .await
        .unwrap();

    RadioControllerThread::start(radio_controller);

    ProtocolUSBThread::start(
        USBDeviceController::new(peripherals.usbd, peripherals.power),
        radio_socket,
        timer.clone(),
    );
    */

    log!("Ready!");

    let mut blink_pin = {
        // if USING_DEV_KIT {
        gpio.pin(pins.P0_15)
            .set_direction(PinDirection::Output)
            .write(PinLevel::Low);

        gpio.pin(pins.P0_14)
        // } else {
        //     gpio.pin(pins.P0_06)
        // }
    };

    blink_pin.set_direction(PinDirection::Output);

    loop {
        blink_pin.write(PinLevel::Low);
        timer.wait_ms(500).await;

        blink_pin.write(PinLevel::High);
        timer.wait_ms(500).await;
    }
}

// TODO: Configure the voltage supervisor.

// TODO: Switch back to returning '!'

entry!(main);
fn main() -> () {
    // Disable interrupts.
    // TODO: Disable FIQ interrupts?
    unsafe { asm!("cpsid i") }

    let mut peripherals = peripherals::raw::Peripherals::new();

    nordic::clock::init_high_freq_clk(&mut peripherals.clock);
    nordic::clock::init_low_freq_clk(
        nordic::clock::LowFrequencyClockSource::CrystalOscillator,
        &mut peripherals.clock,
    );

    // Enabling FPU per:
    // https://developer.arm.com/documentation/ddi0439/b/Floating-Point-Unit/FPU-Programmers-Model/Enabling-the-FPU?lang=en
    //
    // It seems like this must be done after the clocks?
    unsafe {
        asm!("LDR.W   R0, =0xE000ED88");
        asm!("LDR     R1, [R0]");
        asm!("ORR     R1, R1, #(0xF << 20)");
        asm!("STR     R1, [R0]");
    }

    Blinker::start();

    // Enable interrupts.
    unsafe { asm!("cpsie i") };

    loop {
        unsafe { asm!("nop") };
    }
}
