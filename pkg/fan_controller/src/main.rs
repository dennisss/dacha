#![feature(llvm_asm, abi_avr_interrupt)]
#![cfg_attr(target_arch = "avr", no_main)]
#![feature(type_alias_impl_trait)]
#![feature(const_fn)]
#![feature(global_asm)]
#![feature(const_fn_fn_ptr_basics)]
#![cfg_attr(target_arch = "avr", no_std)]
// #![cfg_attr(target_arch = "avr", no_core)]

// extern crate core;

#[macro_use]
pub mod avr;
pub mod protocol;
pub mod usb;

use avr::pins::*;
use avr::registers::*;
use avr::thread;
use avr::*;
use protocol::*;
use usb::*;
use avr::progmem::*;

/*
PC Power Usage:
- Fans: 4 * 1.56W
- Pump: 18W
- GPU: 320W (RTX 3080)
- CPU: 105W (5900X)
    =
*/

// in r25,$16 ; Read Port B
// out $18,r16 ; Write zeros to Port B

// Datasheet:
// https://ww1.microchip.com/downloads/en/DeviceDoc/Atmel-7766-8-bit-AVR-ATmega16U4-32U4_Datasheet.pdf

// NOTE: Should have 1KB internal EEPROM

// DDxn
// - 1 is OUTPUt
// - 0 is INPUT

// PORTxn
// - When input, 1 is PULLUP
// - When output, this is the value

// PINxn
// -

/*
The Status Register is not automatically stored when entering an interrupt routine and restored when returning
from an interrupt. This must be handled by software
*/

// TODO: Enable "ADC Noise Reduction Mode"? (but we probably want the PWMs to
// not stop)

// Should use

/*
    PD0 (INT0) is SCL
    PD1 (INT1) is SDA
    PD2 (INT2) is UART RX
    PD3 (INT3) is UART TX
*/

fn setup() {
    // PB0 - WATER_FLOW : INPUT PCINT0
    // PB1 - ISP_SCK
    // PB2 - ISP_MOSI
    // PB3 - ISP_MISO
    // PB4 - CPU_PWM_IN : INPUT (sample duty cycle with digital reads)
    // PB5 - FAN_PWM_C : OUTPUT OC1A
    // PB6 - FAN_PWM_B : OUTPUT OC1B
    // PB7 - CPU_SPEED_OUT - OUTPUT OC0A regular 20Hz PWM wave (or we could use OC1C
    // to have a 4th Fan pwm output)
    const PORTB_CFG: PortConfig = PortConfig::new()
        .input(0)
        .input(1)
        .input(2)
        .output_low(3)
        .input(4)
        .output_low(5)
        .output_high(6)
        .output_low(7);
    PB::configure(&PORTB_CFG);

    // PC6 - FAN_PWM_A  : OUTPUT OC3A
    // PC7 - LED (Active Low)
    const PORTC_CFG: PortConfig = PortConfig::new().output_low(6).output_high(7);
    PC::configure(&PORTC_CFG);

    // PD0 - FAN_SPEED_4 : INPUT_PULLUP INT0
    // PD1 - FAN_SPEED_3 : INPUT_PULLUP INT1
    // PD2 - FAN_SPEED_2 : INPUT_PULLUP INT2
    // PD3 - FAN_SPEED_1 : INPUT_PULLUP INT3
    // PD4 - LED (Active Low) : OUTPUT
    // PD5 - FPANEL_PLED : INPUT
    // PD6 - FPANEL_POWER : OUTPUT
    // PD7 - FPANEL_RESET : OUTPUT
    const PORTD_CFG: PortConfig = PortConfig::new()
        .input_pullup(0)
        .input_pullup(1)
        .input_pullup(2)
        .input_pullup(3)
        .output_high(4)
        .input(5)
        .output_low(6)
        .output_low(7);
    PD::configure(&PORTD_CFG);

    // PE2: N/C High-Z
    // PE6: FAN_SPEED_5 : INPUT_PULLUP INT6
    const PORTE_CFG: PortConfig = PortConfig::new().input_pullup(6);
    PE::configure(&PORTE_CFG);

    // PF0 - WATER_TEMP : INPUT ADC0
    // PF1 - AIR_TEMP   : INPUT ADC1
    // PF4 - ENABLE_TEMP: OUTPUT (Active High)
    // PF5 - LED (Active Low) : OUTPUT
    // PF6 - LED (Active Low) : OUTPUT
    // PF7 - LED (Active Low) : OUTPUT
    const PORTF_CFG: PortConfig = PortConfig::new()
        .input(0)
        .input(1)
        .output_low(4)
        .output_high(5)
        .output_high(6)
        .output_high(7);
    PF::configure(&PORTF_CFG);

    // Disable digital I/O for ADC0 and ADC1.
    unsafe {
        avr_write_volatile(DIDR0, 0b11);
        avr_write_volatile(DIDR1, 0);
        avr_write_volatile(DIDR2, 0);
    }

    unsafe {
        // Configure all External Interrupts to be falling edge triggered
        avr_write_volatile(EICRA, 0b10101010);
        avr_write_volatile(EICRB, 0b00100000);
        // Enable interrupts for INT0-3,6
        avr_write_volatile(EIMSK, 0b01001111);
    }
}

const FAN_COUNT: usize = 4;

// The max pulse frequency we realistically will see is 100Hz at 3000RPM.
// So if averaging over a few seconds, a u16 should be enough.
const PULSE_NUM_CHANNELS: usize = 6;
static mut PULSE_COUNTS: [u16; PULSE_NUM_CHANNELS] = [0; PULSE_NUM_CHANNELS];

async fn pulse_interrupt_thread(channel: usize, event: InterruptEvent) {
    let pulse_counts = unsafe { &mut PULSE_COUNTS };

    loop {
        event.to_future().await;
        pulse_counts[channel] = pulse_counts[channel].wrapping_add(1);
    }
}

define_thread!(Pulse0Thread, || pulse_interrupt_thread(
    0,
    InterruptEvent::Int3
));
define_thread!(Pulse1Thread, || pulse_interrupt_thread(
    1,
    InterruptEvent::Int2
));
define_thread!(Pulse2Thread, || pulse_interrupt_thread(
    2,
    InterruptEvent::Int1
));
define_thread!(Pulse3Thread, || pulse_interrupt_thread(
    3,
    InterruptEvent::Int0
));
define_thread!(Pulse4Thread, || pulse_interrupt_thread(
    4,
    InterruptEvent::Int6
));
define_thread!(Pulse5Thread, || pulse_interrupt_thread(
    5,
    InterruptEvent::PCInt0
));

// We need to be able to read a 25kHz wave
// So one cycle is 640 system clock cycles.
// We will try to sample 4 cycles and average them (0.16 milliseconds)
fn read_duty_cycle() -> u8 {
    let mut count_high: u16 = 0;

    for i in 0..4 {
        for j in 0..256 {
            if PB4::read() {
                count_high += 1;
            }
        }
    }

    (count_high / 4) as u8
}

/*
    With PCInt0:
    - check '!old_state & new_state' to detect a rising edge
*/

const PWM_NUM_CHANNELS: usize = 3;
const TEMP_NUM_CHANNELS: usize = 2;

#[repr(packed)]
#[derive(Default)]
struct FanControllerState {
    /// Last requested speed (in percent of each fan/pump pwm channel)
    /// 0-255 per PWM channel.
    control: [u8; PWM_NUM_CHANNELS],
    /// Last measured temperatures
    /// Stored as binary coded decimal
    temps: [u16; TEMP_NUM_CHANNELS],
    /// Last measured speeds of each channel
    /// Each value is the number of half revolutations (for fans)
    speeds: [u16; PULSE_NUM_CHANNELS],
    /// Whether or not
    computer_on: bool,
}

static mut STATE: FanControllerState = FanControllerState {
    control: [0; PWM_NUM_CHANNELS],
    temps: [0; TEMP_NUM_CHANNELS],
    speeds: [0; PULSE_NUM_CHANNELS],
    computer_on: false,
};

/*
Settings
- All stored in EEPROM for long term storage with a working copy in SRAM
    - CRC32 used to validate the EEPROM contents
    - ~1.5 seconds to save EEPROM (500 bytes)
- Voltage divider constant for each temperature input
    - 2 * f32 = 8 bytes
- Mode for each connector
    - For each temp sensor, either:
        - PRIMARY TEMP, SECONDARY TEMP, OFF
    - For each fan connector, either:
        - ON or OFF
    - For the CPU connector
        - CPU, FAN ON or totally off
        - We may want to re-use the CPU connector as a fan controller
- Fan curves * 3 (optionally up to * 4 if using CPU as fan)
    - Each is 0 to 100 degrees
        - 0-100% fan speed at each degree
        - Top bit used to indicate if this is a user set-point
- Options
    - Scale fan is one fan in a pair turns off.
    - Enable SPI interface
*/
#[repr(packed)]
struct FanControllerSettings {
    checksum: u32,
    /// Scaler which if multiplied by the voltage on a temperature pin will
    /// produce the temperature in degrees Celsius.
    temp_voltage_scaler: [f32; TEMP_NUM_CHANNELS],
    control_curves: [[u8; 100]; PWM_NUM_CHANNELS],
}

define_thread!(MainThread, main_thread);
async fn main_thread() -> () {
    loop {
        // Get initial time
        delay_ms(2000).await;
        // Get final time

        /*
        let state = unsafe { &mut STATE };
        state.computer_on = PD5::read();
        */

        // TODO: Calibrate temperatures?
        // Read temperatures
        let value = ADC::read(ADCInput::ADC0).await;

        // Compute new control inputs and set them (and store them in state)

        // Mark the state as ready to be sent back to the host (also generate a
        // message queue event).
    }
}

// TODO: Verify that these are stored in flash only to save space.
// See also http://www.nongnu.org/avr-libc/user-manual/group__avr__pgmspace.html#ga88d7dd4863f87530e1a34ece430a587c
// Needs to use the 'lpm' instruction.
progmem!(DEVICE_DESC: DeviceDescriptor = DeviceDescriptor {
    bLength: core::mem::size_of::<DeviceDescriptor>() as u8,
    bDescriptorType: DescriptorType::DEVICE as u8,
    bcdUSB: 0x0200, // 2.0
    bDeviceClass: 0,
    bDeviceSubClass: 0,
    bDeviceProtocol: 0,
    bMaxPacketSize0: 64,
    idVendor: 0x8888,
    idProduct: 0x0001,
    bcdDevice: 0x0100, // 1.0,
    iManufacturer: 0,
    iProduct: 0,
    iSerialNumber: 0,
    bNumConfigurations: 1,
});

progmem!(CONFIG_DESC: ConfigurationDescriptor = ConfigurationDescriptor {
    bLength: core::mem::size_of::<ConfigurationDescriptor>() as u8,
    bDescriptorType: DescriptorType::CONFIGURATION as u8,
    // TODO: Make this field more maintainable.
    wTotalLength: (core::mem::size_of::<ConfigurationDescriptor>()
        + core::mem::size_of::<InterfaceDescriptor>()
        + 2 * core::mem::size_of::<EndpointDescriptor>()) as u16,
    bNumInterfaces: 1,
    bConfigurationValue: 1,
    iConfiguration: 0,
    // TODO: Double check this
    bmAttributes: 0xa0, // Bus Powered : Remote wakeup
    bMaxPower: 50,
});

progmem!(IFACE_DESC: InterfaceDescriptor = InterfaceDescriptor {
    bLength: core::mem::size_of::<InterfaceDescriptor>() as u8,
    bDescriptorType: DescriptorType::INTERFACE as u8,
    bInterfaceNumber: 0,
    bAlternateSetting: 0,
    bNumEndpoints: 2,
    bInterfaceClass: 0, // TODO
    bInterfaceSubClass: 0,
    bInterfaceProtocol: 0,
    iInterface: 0,
});

progmem!(EP1_DESC: EndpointDescriptor = EndpointDescriptor {
    bLength: core::mem::size_of::<EndpointDescriptor>() as u8,
    bDescriptorType: DescriptorType::ENDPOINT as u8,
    bEndpointAddress: 0x81, // EP IN 1
    bmAttributes: 0b11,     // Interrupt
    wMaxPacketSize: 64,
    bInterval: 64, // TODO: Check me.
});

progmem!(EP2_DESC: EndpointDescriptor = EndpointDescriptor {
    bLength: core::mem::size_of::<EndpointDescriptor>() as u8,
    bDescriptorType: DescriptorType::ENDPOINT as u8,
    bEndpointAddress: 0x02, // EP OUT 2
    bmAttributes: 0b11,     // Interrupt
    wMaxPacketSize: 64,
    bInterval: 64, // TODO: Check me.
});

pub trait USBDescriptorSet {
    fn device(&self) -> &'static ProgMem<DeviceDescriptor>;
    fn config(&self, index: usize) -> Option<&'static ProgMem<ConfigurationDescriptor>>;
    fn interface(
        &self,
        config_index: usize,
        iface_index: usize,
    ) -> Option<&'static ProgMem<InterfaceDescriptor>>;
    fn endpoint(
        &self,
        config_index: usize,
        iface_index: usize,
        index: usize,
    ) -> Option<&'static ProgMem<EndpointDescriptor>>;
}

struct FanControllerUSBDesc {}
impl USBDescriptorSet for FanControllerUSBDesc {
    fn device(&self) -> &'static ProgMem<DeviceDescriptor> {
        &DEVICE_DESC
    }
    fn config(&self, index: usize) -> Option<&'static ProgMem<ConfigurationDescriptor>> {
        match index {
            0 => Some(&CONFIG_DESC),
            _ => None,
        }
    }
    fn interface(
        &self,
        config_index: usize,
        iface_index: usize,
    ) -> Option<&'static ProgMem<InterfaceDescriptor>> {
        match iface_index {
            0 => Some(&IFACE_DESC),
            _ => None,
        }
    }
    fn endpoint(
        &self,
        config_index: usize,
        iface_index: usize,
        index: usize,
    ) -> Option<&'static ProgMem<EndpointDescriptor>> {
        match index {
            0 => Some(&EP1_DESC),
            1 => Some(&EP2_DESC),
            _ => None,
        }
    }
}

// TODO: Instead have an iterator that returns labelled slices so that we can
// support marking if something is in program memory.

struct USBConfigDescIter<'a> {
    set: &'a dyn USBDescriptorSet,
    config_index: usize,
    current_index: USBConfigDescIndex,

    // TODO: Should use a progmem iter
    remaining: ProgMemIterBytes,
}

enum USBConfigDescIndex {
    Config,
    Interface(usize),
    Endpoint(usize, usize),
}

impl<'a> USBConfigDescIter<'a> {
    fn new(set: &'a dyn USBDescriptorSet, config_index: usize) -> Self {
        Self {
            set,
            config_index,
            current_index: USBConfigDescIndex::Config,
            // NOTE: Must have at least one item.
            remaining: set.config(config_index).unwrap().iter_bytes(),
        }
    }
}

impl<'a> core::iter::Iterator for USBConfigDescIter<'a> {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(v) = self.remaining.next() {
                return Some(v);
            }

            // Increment index
            match self.current_index {
                USBConfigDescIndex::Config => {
                    if let Some(new_remaining) = self.set.interface(self.config_index, 0) {
                        self.current_index = USBConfigDescIndex::Interface(0);
                        self.remaining = new_remaining.iter_bytes();
                    } else {
                        return None;
                    }
                }
                USBConfigDescIndex::Interface(n) => {
                    if let Some(new_remaining) = self.set.endpoint(self.config_index, n, 0) {
                        self.current_index = USBConfigDescIndex::Endpoint(n, 0);
                        self.remaining = new_remaining.iter_bytes();
                    } else if let Some(new_remaining) = self.set.interface(self.config_index, n + 1)
                    {
                        self.current_index = USBConfigDescIndex::Interface(n + 1);
                        self.remaining = new_remaining.iter_bytes();
                    } else {
                        return None;
                    }
                }
                USBConfigDescIndex::Endpoint(iface_num, ep_num) => {
                    // TODO: Deduplicate with the above case!

                    if let Some(new_remaining) =
                        self.set.endpoint(self.config_index, iface_num, ep_num + 1)
                    {
                        self.current_index = USBConfigDescIndex::Endpoint(iface_num, ep_num + 1);
                        self.remaining = new_remaining.iter_bytes();
                    } else if let Some(new_remaining) =
                        self.set.interface(self.config_index, iface_num + 1)
                    {
                        self.current_index = USBConfigDescIndex::Interface(iface_num + 1);
                        self.remaining = new_remaining.iter_bytes();
                    } else {
                        return None;
                    }
                }
            }
        }
    }
}

// Need at least one thread to handle USB resets?
// Such a thread can also restart other threads assuming they aren't doing
// anything important?

define_thread!(
    /// Resets all USB state upon host reset requests.
    USBResetThread,
    usb_reset_thread
);
async fn usb_reset_thread() -> () {
    // TODO: Need to properly order detaching the USB device:
    // - First enable the END_OF_RESET interrupt handler
    // - Then attach the USB device
    // Then synchronously start waiting for the interrupt.
    // - this is especially complicated if the controller state is reset

    unsafe {
        avr_write_volatile(UDCON, 1 /* | 1 << 2 */)
    };
    delay_ms(1000).await;

    // Attach the USB.
    unsafe {
        avr_write_volatile(UDCON, 0 /* | 1 << 2 */)
    };

    loop {
        wait_usb_end_of_reset().await;

        // Stop all threads
        // TODO: This should be unsafe as a thread shouldn't be allowed to stop
        // itself. USBControlThread::stop();
        // USBRxThread::stop();
        // USBTxThread::stop();

        // Reconfigure all endpoints.
        // TODO: Verify that this properly resets all of the usb controller
        // state.

        avr::usb::init_endpoints();

        // (Re-)start all threads
        USBControlThread::start();
        // USBRxThread::start();
        // USBTxThread::start();

        USART1::send_blocking(b"RESET!\n");
    }
}

// TODO: MAke this unsafe.
fn struct_bytes<'a, T>(v: &'a T) -> &'a [u8] {
    unsafe {
        core::slice::from_raw_parts(
            core::mem::transmute::<&T, *const u8>(v),
            core::mem::size_of::<T>(),
        )
    }
}

unsafe fn struct_bytes_mut<'a, T>(v: &'a mut T) -> &'a mut [u8] {
    core::slice::from_raw_parts_mut(
        core::mem::transmute::<_, *mut u8>(v),
        core::mem::size_of::<T>(),
    )
}

const ADDEN: u8 = 7;

fn read_setup(pkt: &mut SetupPacket) -> bool {
    let pkt_buf = unsafe { struct_bytes_mut(pkt) };
    let bytec: u16 = USB_EP0.bytec();

    // On error, just perform a STALL
    if bytec != (pkt_buf.len() as u16) || bytec != 8 {
        return false;
    }

    // NOTE: If pkt.wLength == 0, then there is no data stage and thus no need to
    // send data.
    USB_EP0.read_bytes(pkt_buf);
    drop(pkt_buf);

    USB_EP0.clear_setup();
    true
}

define_thread!(
    /// Handles control packets on USB Endpoint 0.
    /// e.g. returning descriptors.
    USBControlThread,
    usb_control_thread
);
async fn usb_control_thread() -> () {
    // USART1::send_blocking(b"START CONTROL\n");

    // TODO: Do I need to do anything special to ensure that

    // loop {
    //     PD5::write(false);
    //     delay_ms(500).await;
    //     PD5::write(true);
    //     delay_ms(500).await;
    // }

    const EP: &'static USBEndpoint = &USB_EP0;
    let mut pkt = SetupPacket::default();

    loop {
        EP.wait_setup().await;

        if !read_setup(&mut pkt) {
            USART1::send_blocking(b"E0\n");
            EP.request_stale();
            continue;
        }

        if pkt.bRequest == StandardRequestType::SET_ADDRESS as u8 {
            if pkt.bmRequestType != 0b00000000 || pkt.wIndex != 0 || pkt.wLength != 0 {
                EP.request_stale();
                // USART1::send_blocking(b"E1\n");
                continue;
            }

            if unsafe { avr_read_volatile(UDADDR) } != 0 {
                EP.request_stale();
                // USART1::send_blocking(b"E2\n");
                continue;
            }

            let addr = (pkt.wValue & 0x7f) as u8;
            // Store address. Not enabled yet
            unsafe { avr_write_volatile(UDADDR, addr) };

            // NOTE: No data should be received for this request

            // IN: status
            EP.wait_transmitter_ready().await;
            EP.clear_transmitter_ready();

            // Wait for the status stage to be completed (buffer flushed) before enabling
            // the address.
            EP.wait_transmitter_ready().await;

            // Enable address
            unsafe { avr_write_volatile(UDADDR, avr_read_volatile(UDADDR) | 1 << ADDEN) };
        } else if pkt.bRequest == StandardRequestType::SET_CONFIGURATION as u8 {
            if pkt.bmRequestType != 0b00000000 {
                EP.request_stale();
                // USART1::send_blocking(b"E3\n");
                continue;
            }

            // TODO: upper byte of wValue is reserved.
            // TODO: Value of 0 puts device in address state.

            if pkt.wValue != 1 {
                EP.request_stale();
                USART1::send_blocking(b"SET CFG FAIL\n");
                continue;
            }

            // No data stage

            // Status stage
            // TODO: This is standard from any 'Host -> Device' request
            EP.wait_transmitter_ready().await;
            EP.clear_transmitter_ready();
        } else if pkt.bRequest == StandardRequestType::GET_CONFIGURATION as u8 {
            if pkt.bmRequestType != 0b10000000 || pkt.wValue != 0 || pkt.wIndex != 0 || pkt.wLength != 1 {
                EP.request_stale();
                // USART1::send_blocking(b"E4\n");
                continue;
            }

            EP.control_respond(&pkt, (&[1]).iter().cloned()).await;
        } else if pkt.bRequest == StandardRequestType::GET_DESCRIPTOR as u8 {
            if pkt.bmRequestType != 0b10000000 {
                EP.request_stale();
                // USART1::send_blocking(b"E5\n");
                continue;
            }

            let desc_type = (pkt.wValue >> 8) as u8;
            let desc_index = (pkt.wValue & 0xff) as u8; // NOTE: Starts at 0

            if desc_type == DescriptorType::DEVICE as u8 {
                let data = struct_bytes(&DEVICE_DESC);
                if desc_index != 0 {
                    EP.request_stale();
                    USART1::send_blocking(b"EX\n");
                    continue;
                }
                // TODO: Assert language code.

                // avr_assert_eq!(pkt.wLength, data.len() as u16);
                EP.control_respond(&pkt, data.iter().cloned()).await;
            } else if desc_type == DescriptorType::CONFIGURATION as u8 {
                let inst = FanControllerUSBDesc {};

                // TODO: Validate that the configuration exists.
                // If it doesn't return an error.
                let iter = USBConfigDescIter::new(&inst, desc_index as usize);
                EP.control_respond(&pkt, iter).await;
            } else if desc_type == DescriptorType::ENDPOINT as u8 {
                let inst = FanControllerUSBDesc {};
                // TODO: Support different intervales.
                let iter = inst.endpoint(0, 0, desc_index as usize).unwrap();
                EP.control_respond(&pkt, unsafe { struct_bytes(iter) }.iter().cloned())
                    .await;
            } else if desc_type == DescriptorType::DEVICE_QUALIFIER as u8 {
                // According to the USB 2.0 spec, a full-speed only device should respond to
                // a DEVICE_QUALITY request with an error.
                //
                // TODO: Probably simpler to just us the USB V1 in the device descriptor?
                EP.request_stale();
            } else {
                EP.request_stale();
            }
        } else {
            EP.request_stale();
        }
    }

    /* If a command is not supported or contains
    an error, the firmware set the STALL request flag and can return to the main task, waiting for the next SETUP
    request. */

    // TODO: Possibly set STALLRQ on errors?
}

// Must be able to recorver from

/// Header sent with every USB packet.
#[repr(packed)]
#[derive(Default)]
struct FanControllerPacketHeader {
    typ: u8,
    /// Total length of all data (excluding packet headers).
    /// May span multiple USB packets.
    payload_size: u16,
}

///
static RESPONSE_CHANNEL: Channel<[u8; 64]> = Channel::new();

// This thread is waiting for commands to be received.
define_thread!(
    /// Receives and executes commands from the host device.
    USBRxThread,
    usb_rx_thread
);
async fn usb_rx_thread() -> () {
    const EP: &'static USBEndpoint = &USB_EP1;
    let mut header = FanControllerPacketHeader::default();

    // If an error occurs, then we should consume all request OUT packets until we
    // are allowed to send something.

    loop {
        EP.wait_received_data().await;

        let n = EP.bytec();
        if n < 3 {
            // Stall!
        }

        EP.read_bytes(unsafe { struct_bytes_mut(&mut header) });

        if header.typ == FanControllerPacketType::PressPower as u8 {
            if EP.bytec() != 2 || header.payload_size != 2 {
                // Error out!
            }

            // Assert n == 5
            // Verify length in

            let mut payload = [0u8; 2];
            EP.read_bytes(&mut payload);

            let time_ms = u16::from_le_bytes(payload);

            PD6::write(true);
            delay_ms(time_ms).await;
            PD6::write(false);
        }

        // TODO: At the end of each packet, don't forgeth to flip to the other
        // FIFO bank.

        /*
        match header.typ {
            FanControllerPacketType::GetSettings as u8 => {
                avr_assert_eq!(header.payload_size, 0);

                // Loop through settings and continuously append to

                // Challenges:
                // - Must acquire a USB 'lock', because the protocol can't MUX different message types
                // - locks need to be short lived to allow for

            },
            FanControllerPacketType::SetSettings as u8 => {
                avr_assert_eq!(header.payload_size, core::mem::size_of::<FanControllerSettings>());

                // Acquire lock to have exclusive access to the in-memory settings.

                // Read into that space from USB with a 2 second timeout

                // Unlock

                // Flush to EEPROM

                // Response to USB request

            }
        }
        */

        // TODO: For now, only accept 64byte packets.
    }

    // Wait for an interrupt on EP2 that is RX related.
    // NOTE:

    // TODO: We should only enable USB interrupts as soon as we start waiting
    // for them! Although some will also auto-trigger
}

// This thread waits for in-RAM queues to fill up and then sends
define_thread!(
    /// Sends locally available packets back to the host.
    USBTxThread,
    usb_tx_thread
);
async fn usb_tx_thread() -> () {
    // RAM queues (in order of sending priority):
    // P0: Printing debug info
    // P1: State available
    // P2: Responses to RX'ed commands
}

// Becuase of this, we should try to yield time to other threads if this occurs:
// TODO: Enable interrupts after each byte read/wrirten
// When the EEPROM is read, the CPU is halted for four clock cycles before the
// next instruction is executed. When the EEPROM is written, the CPU is halted
// for two clock cycles before the next instruction is executed.

define_thread!(TestThread, test_thread);
#[no_mangle]
#[inline(never)]
async fn test_thread() {
    loop {
        PB0::write(false);
        delay_ms(1000).await;
        // InterruptEvent::Int0.to_future().await;
        // testing_inner(20).await;

        PB0::write(true);
        delay_ms(1000).await;
        // InterruptEvent::Int0.to_future().await;
        // testing_inner(20).await;
    }
}

// TODO: Need to verify that interrupt flags aren't cleared until after the
// interupt has finished executing (otherwise we need to depend on not reading
// that flag),

#[cfg(target_arch = "avr")]
#[no_mangle]
pub extern "C" fn main() {
    avr::init();
    USART1::init();
    avr::usb::init_endpoints();

    // // TODO: Document whether or not pins start high or low.
    const PORTB_CFG: PortConfig = PortConfig::new().output_high(0);
    PB::configure(&PORTB_CFG);

    const PORTD_CFG: PortConfig = PortConfig::new()
        .input_pullup(0)
        .output_high(3)
        .output_high(5);
    PD::configure(&PORTD_CFG);

    unsafe {
        // Configure all External Interrupts to be falling edge triggered
        avr_write_volatile(EICRA, 0b10101010);
        avr_write_volatile(EICRB, 0b00100000);
        // Enable interrupts for INT0-3,6
        avr_write_volatile(EIMSK, 1);

        // Clear initial interrupt bits.
        avr_write_volatile(EIFR, 0);
    }

    // TODO: Probably need to wait some amount of time before we can send the first
    // bit.
    USART1::send_blocking(b"START!\n");

    TestThread::start();

    USBResetThread::start();

    // NOTE: Linux/Windows will read the DEVICE descriptor before triggering the
    // first reset, so this thread must be started even before the first reset.
    USBControlThread::start();

    // USBTxThread::start();
    // USBRxThread::start();

    // Pulse0Thread::start();
    // Pulse1Thread::start();
    // Pulse2Thread::start();
    // Pulse3Thread::start();
    // Pulse4Thread::start();
    // Pulse5Thread::start();

    avr::thread::block_on_threads();

    /*
    setup();
    // TODO: Configure SREG

    MainThread::start();

    // Also some USB threads.

    avr::thread::block_on_threads();
    */
}
