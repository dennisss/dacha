/*
List of pins:
- Each pin:
    - Static:
        - Port num
        - Pin num
    - Dynamic
        - Bit of whether or not it is in use.


GPIO peripheral is pretty straight forward:
- Need a list of slots which we can configure



cargo run --bin builder --  build //pkg/nordic:nordic_radio_dongle --config=//pkg/nordic:nrf52840
cargo run --bin flasher built/pkg/nordic/nordic_radio_dongle uf2-dfu


Target clock speed:

- RP2040 for 1 stepper achieves
    - '(5 / 12000000) seconds' per step in klipper
    - '(22 / 12000000) seconds' per step for 3 steppers


Useful FICR stuff:


- INFO.PART
    - To verify we are the right MCU
    - Should be '0x52840'

- INFO.VARIANT
    - Weird format??


- INFO.DEVICEID[0] and INFO.DEVICEID[1]
    - 64-bit unique id which is convenient.

- DEVICEADDR[0] and [1] are a 48-bit addr.


TODO: Even If USB resets don't reset the MCU, I need to use them to reset the USB streams since framing will be messed up if there is old unread data.

*/

use common::fixed::vec::FixedVec;
use common::cyclic_buffer::CyclicBuffer;
use common::list::Appendable;
use executor::sync::AsyncMutex;
use common::register::RegisterRead;
use executor::critical_mutex::{CriticalMutex, Interruptable};
use executor::channel::Channel;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode, ConfigureGPIO_InterruptPolarity,
    PeripheralResponseResponseCase
};
use peripherals::raw::gpiote::GPIOTE;
use peripherals::raw::saadc::SAADC;
use peripherals::raw::pwm0::PWM0;
use peripherals::raw::pwm1::PWM1;
use peripherals::raw::pwm2::PWM2;
use peripherals::raw::pwm3::PWM3;
use peripherals::raw::spim0::SPIM0;
use peripherals::raw::spim1::SPIM1;
use peripherals::raw::spim2::SPIM2;
use peripherals::raw::spim3::SPIM3;
use peripherals::raw::temp::TEMP;
use peripherals::raw::timer0::TIMER0;
use peripherals::raw::timer1::TIMER1;
use peripherals::raw::timer2::TIMER2;
use peripherals::raw::timer3::TIMER3;
use peripherals::raw::timer4::TIMER4;
use peripherals::raw::uarte0::UARTE0;
use peripherals::raw::uarte1::UARTE1;
use peripherals::raw::ppi::PPI;
use peripherals::raw::twim0::TWIM0;
use protobuf::{Message, StaticMessage};
use executor::interrupts::make_high_priority_irq;
use peripherals::raw::Interrupt;

use crate::gpio::*;
use crate::gpiote::{GPIOTEChannels, GPIOInterruptChannel, GPIOInterruptPolarity};
use crate::pins::{PeripheralPin, Port};
use crate::pwm::{PWMConfig, PWM};
use crate::rtc::{RTCInstant, RTC};
use crate::temp::Temp;
use crate::uarte::*;
use crate::timer::{Timer, TimerChannel, TIMERx};
use crate::ppi::*;
use crate::spi::*;
use crate::neopixel::*;
use crate::adc::*;
use crate::twim::*;
use crate::radio::*;

use super::time::*;
use super::neopixel::*;
use super::tachometer::TachometerPeripheralThread;
use super::temp::TemperaturePeripheralThread;
use super::timeout::TimeoutPeripheralThread;
use super::uart::UartTransmitPeripheralThread;
use super::stepper::{StepperMotorController};
use super::interrupt::InterruptPeripheralThread;
use super::adc::*;
use super::spi::SPIPeripheralThread;
use super::sleep::SleepPeripheralThread;
use super::i2c::I2CPeripheralThread;
use super::buffer::Buffer;
use super::radio::*;
use super::allocator::BoxedSlice;
use super::timer_controller::*;
use super::spi_timer_controller::*;

const MAX_NUM_PERIPHERALS: usize = 32;

// Port 0 has 32 pins.
// Port 1 has 16 pins.
//
// TODO: Update based on MCU type.
const NUM_PINS: usize = 48;

const RESPONSE_BUFFER_SIZE: usize = 512;

const DEFAULT_ENTRY_VALUE: PeripheralEntry = PeripheralEntry::Unconfigured;


pub struct PeripheralsController {
    pub(super) clock: RTC,
    
    /// This state is interruptable since the only higher priority interrupts only access the
    /// TimerController state which has a separate lock.
    pub(super) state: CriticalMutex<PeripheralsControllerState, Interruptable>,
    
    // TODO: Give this a dedicated interrupt.
    response_buffer_readable: Channel<()>,

    pub(super) adc_request_queue_filled: Channel<()>,

    // NOTE: Locking the state is still recommended for this to avoid multiple users taking the same channel slot.
    pub(super) timer: Timer,

    pub(super) timer_controller: TimerController,
}

// TODO: Unconfigure all peripherals when this is dropped.
pub(super) struct PeripheralsControllerState {
    pub entries: [PeripheralEntry; MAX_NUM_PERIPHERALS],

    /// Has a bit set for every pin that is in use by one of the entries.
    ///
    /// NOTE: While this is derivable from 'entries', we can't derive it when
    /// entries are a 'Borrowed' state
    pub used_pins: [u32; NUM_PINS / 4],

    /// When true, all pins/peripherals have been configured. More peripherals
    /// can't be configured unless we reset the current config.
    pub config_finalized: bool,

    pub pwms: [PWM; 4],

    pub gpio: GPIO,

    pub gpiote: GPIOTEChannels,

    pub ppi: PPIChannels,

    pub temp: Option<Temp>,

    pub uarte: FixedVec<UARTEx, 2>,

    spim: FixedVec<SPIMx, 4>,

    /// Extra unused timers.
    pub timers: FixedVec<TIMERx, 3>,

    pub adc: Option<WindowADC>,

    pub adc_request_queue: ADCRequestQueue,

    pub radio: Option<Radio>,

    twim0: Option<TWIM0>,

    batch_ack: Option<BatchAck>,

    // NOTE: THis always has a value after fully configured.
    usb_sof_tracker: Option<TimedEvent>,

    /// Stores PeripheralResponse protos that need to be read back by the host.
    //
    // TODO: Clear this on USB reset.
    //
    // TODO: Just use a cyclic buffer. If we overflow, wait for everything to be read and then write
    // an overflow message.
    response_buffer: CyclicBuffer<[u8; RESPONSE_BUFFER_SIZE]>,

    // TODO: While overflowed, we need to surpress responses until we have space to say that we overflowed.
    response_buffer_overflowed: bool,
}

struct BatchAck {
    first_sequence: u32,
    num_requests: u32,
}

pub(super) enum PeripheralEntry {
    Unconfigured,

    /// A thread is currently operating on this peripheral and has exclusive
    /// access to it.
    Borrowed,

    GPIO(GPIOEntry),
    PWM {
        index: usize,
        channel: usize,
        inverted: bool,
        default_value: u16,
        last_active: Option<RTCInstant>,
        timeout_millis: Option<u32>,
    },

    UARTE(UARTE),

    /// The value is the index of the entry in the TimerController
    Stepper(usize),

    Neopixel(NeopixelPeripheral),

    ADC(WindowADCChannelConfig),

    Buffer(Buffer),

    SPI(SPIHost),

    SPITimer(usize),

    I2C(TWIM),

    Radio(RadioEntry),
}

pub(super) struct GPIOEntry {
    pub pin: GPIOPin,
    pub interrupt_polarity: ConfigureGPIO_InterruptPolarity,
    pub pending_interrupt_sequence: Option<u32>,
    pub tachometer: Option<GPIOTachometerState>,
}

pub(super) struct GPIOTachometerState {
    interrupt: GPIOInterruptChannel,
    timer: TIMERx,
    ppi: PPIChannel,
}

macro_rules! check_peripheral_type {
    ($state:ident, $peripheral_idx:expr, $t:ident) => {
        match &mut $state.entries[$peripheral_idx] {
            PeripheralEntry::$t { .. } => {}
            _ => {
                return Err(ExecuteError::ErrorCode(
                    PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                ));
            }
        };
    }
}

macro_rules! borrow_peripheral {
    ($state:ident, $peripheral_idx:expr, $t:ident) => {
        {
            let mut e = PeripheralEntry::Borrowed;
            core::mem::swap(&mut e, &mut $state.entries[$peripheral_idx]);
            match e {
                PeripheralEntry::$t(inst) => inst,
                _ => panic!(),
            }
        }
    }
}

macro_rules! get_peripheral_mut {
    ($state:ident, $peripheral_idx:expr, $t:ident) => {
        {
            match &mut $state.entries[$peripheral_idx] {
                PeripheralEntry::$t(entry) => entry,
                _ => {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                    ));
                }
            }
        }
    }
}

impl Default for PeripheralEntry {
    fn default() -> Self {
        Self::Unconfigured
    }
}

pub(super) struct IndexedPin {
    index: u32,
}

impl PeripheralPin for IndexedPin {
    fn pin(&self) -> u8 {
        (self.index % 32) as u8
    }

    fn port(&self) -> crate::pins::Port {
        if self.index >= 32 {
            Port::P1
        } else {
            Port::P0
        }
    }
}

enum ExecuteError {
    AsyncReply,
    ErrorCode(PeripheralResponse_ErrorCode),
}

struct OkResponse;

impl PeripheralsController {
    pub fn new(
        clock: RTC,
        pwm0: PWM0,
        pwm1: PWM1,
        pwm2: PWM2,
        pwm3: PWM3,
        spim0: SPIM0,
        spim1: SPIM1,
        spim2: SPIM2,
        spim3: SPIM3,
        gpiote: GPIOTE,
        temp: TEMP,
        uarte0: UARTE0,
        uarte1: UARTE1,
        timer0: TIMER0,
        timer1: TIMER1,
        timer2: TIMER2,
        timer3: TIMER3,
        timer4: TIMER4,
        ppi: PPI,
        saadc: SAADC,
        twim0: TWIM0,
        radio: Radio,
    ) -> Self {

        // The stepper interrupt.
        // NOTE: The usefulness of this relies on the
        // PeripheralControllerState having interrupts disabled when locked. Overwise, this high pri interrupt may interrupt when it is locked and have to retry after a pendsv interrupt.
        make_high_priority_irq(Interrupt::TIMER4);

        // TODO: Don't create this here. We should ban calling this outside of main().
        let mut peripherals = peripherals::raw::Peripherals::new();
        let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);
        let timer = Timer::new(timer4.into());

        let entries = [DEFAULT_ENTRY_VALUE; MAX_NUM_PERIPHERALS];

        // TODO: Instead init from a slice.
        let mut spim = FixedVec::new();
        spim.push(spim0.into());
        spim.push(spim1.into());
        spim.push(spim2.into());
        spim.push(spim3.into());

        let mut ppi = PPIChannels::new(ppi);

        let adc = WindowADC::create(ADC::new(saadc), timer1, &mut ppi, clock.clone()).unwrap();

        let mut uarte = FixedVec::new();
        uarte.push(uarte0.into());
        uarte.push(uarte1.into());

        let mut timers = FixedVec::new();
        timers.push(timer0.into());
        timers.push(timer2.into());
        timers.push(timer3.into());

        Self {
            clock,
            timer,
            timer_controller: TimerController::new(),
            state: CriticalMutex::new(PeripheralsControllerState {
                entries,
                config_finalized: false,
                pwms: [
                    PWM::new(pwm0.into()),
                    PWM::new(pwm1.into()),
                    PWM::new(pwm2.into()),
                    PWM::new(pwm3.into()),
                ],
                uarte,
                gpio,
                gpiote: GPIOTEChannels::new(gpiote),
                ppi,
                temp: Some(Temp::new(temp)),
                used_pins: [0; NUM_PINS / 4],
                spim,
                twim0: Some(twim0),
                adc: Some(adc),
                timers,
                adc_request_queue: ADCRequestQueue::default(),
                batch_ack: None,
                response_buffer: CyclicBuffer::new([0u8; RESPONSE_BUFFER_SIZE]),
                response_buffer_overflowed: false,
                radio: Some(radio),
                usb_sof_tracker: None,
            }),
            response_buffer_readable: Channel::new(),
            adc_request_queue_filled: Channel::new(),
        }
    }

    pub fn start(&'static self) {
        TimeoutPeripheralThread::start(self);
        TimerControllerResponseThread::start(self);

        unsafe {
            executor::interrupts::override_interrupt_handler(
                Interrupt::TIMER4,
                timer_controller_interrupt,
                core::mem::transmute(&self.timer_controller)
            );
        }
    }

    /// TODO: This requires that 'self' is pinned.
    pub fn execute(&'static self, request: &PeripheralRequest) {
        let mut res = PeripheralResponse::default();

        lock!(state <= self.state.lock(), {
            let response_ready = match self.execute_impl(&mut state, request, &mut res) {
                Ok(OkResponse) => true,
                Err(ExecuteError::ErrorCode(code)) => {
                    res.set_error_code(code);
                    true
                }
                Err(ExecuteError::AsyncReply) => false,
            };

            // TODO: Need to complain if we end up overflowing the buffer.
            if response_ready {
                res.set_request_sequence(request.request_sequence());
                self.write_response(&mut state, &res);
            }
        });
    }

    /// Should be called when the USB connection is reset.
    /// This event means that all previous request data needs to be invalidated.
    pub async fn handle_reset(&self) {
        // TODO: Currently this isn't fool proof since if there are since any running
        // requests then they may still append to the buffer shortly after it is cleared
        // with stale request ids.
        lock!(state <= self.state.lock(), {
            state.batch_ack = None;
            state.response_buffer.clear();
            state.response_buffer_overflowed = false;
        });
    }

    /// Reads response data to be sent back to the host.
    /// Each response is prefixed by a 1 byte length field so that
    /// multiple responses can be send back at once. This can also
    /// be safely padded with zeros if needed. 
    pub fn read_response(&self, out: &mut [u8]) -> usize {
        lock!(state <= self.state.lock(), {
            if state.response_buffer_overflowed && state.response_buffer.is_empty() {
                state.response_buffer_overflowed = false;
                out[0] = 0;
                return 1;
            }
            
            // Putting this here is probably fine as the host will always write
            // all requests before getting responses 
            if let Some(batch_ack) = state.batch_ack.take() {
                self.write_batch_ack(&mut state, batch_ack);
            }

            state.response_buffer.read(out)
        })
    }

    pub async fn wait_until_readable(&self) {
        loop {
            let state = self.state.lock();

            if state.response_buffer.len() > 0 || state.batch_ack.is_some() || state.response_buffer_overflowed {
                return;
            }

            drop(state);

            let _ = self.response_buffer_readable.recv().await;
        }
    }

    /// Returns:
    /// - Ok(OkResponse) if the command is done and ready to send back a
    ///   response.
    /// - Err(ErrorCode(_)) if the command is done and needs to send back an
    ///   error.
    /// - Err(AsyncReply) if the command is still running and will
    ///   asynchronously write a response.
    fn execute_impl(
        &'static self,
        state: &mut PeripheralsControllerState,
        request: &PeripheralRequest,
        response: &mut PeripheralResponse,
    ) -> Result<OkResponse, ExecuteError> {
        let peripheral_idx = request.peripheral_index() as usize;

        if peripheral_idx >= state.entries.len() {
            return Err(ExecuteError::ErrorCode(
                PeripheralResponse_ErrorCode::PERIPHERAL_OUT_OF_RANGE,
            ));
        }

        // NOTE: Commands should avoid mutating the state until they have verified they
        // won't return an error to avoid having partial state updates.
        match request.command_case() {
            PeripheralRequestCommandCase::NOT_SET => Err(ExecuteError::ErrorCode(
                PeripheralResponse_ErrorCode::UNSUPPORTED_COMMAND,
            )),
            PeripheralRequestCommandCase::ConfigureGpio(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;
                self.check_valid_pin(req.pin())?;

                // TODO: Check the pin is unused.

                // TODO: Mark the pin as in use.

                let mut pin = state.gpio.pin(IndexedPin { index: req.pin() });

                // TODO: Standardize doing this more.
                pin.reset();

                if !req.is_input() {
                    pin.write_bool(req.default_value());
                }

                pin.set_direction(if req.is_input() {
                    PinDirection::Input
                } else {
                    PinDirection::Output
                });
                pin.set_resistor(if req.pull_down() {
                    Resistor::PullDown
                } else if req.pull_up() {
                    Resistor::PullUp
                } else {
                    Resistor::None
                });

                // TODO: Also need to support NRF52 high drive.

                state.entries[peripheral_idx] = PeripheralEntry::GPIO(GPIOEntry {
                    pin,
                    interrupt_polarity: req.interrupt(),
                    pending_interrupt_sequence: None,
                    tachometer: None,
                });

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::ConfigurePwm(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;
                self.check_valid_pin(req.pin())?;

                // TODO: Check all the usual stuff.

                // TODO: Reserve the pin.

                let pwm_config = PWMConfig::from_frequency(req.frequency()).ok_or_else(|| {
                    ExecuteError::ErrorCode(PeripheralResponse_ErrorCode::UNSUPPORTED_CONFIG)
                })?;

                let mut pwm_index = None;

                for i in 0..state.pwms.len() {
                    if state.pwms[i].has_connected_pins() {
                        if pwm_config != state.pwms[i].config() || !state.pwms[i].has_available_channel() {
                            continue;
                        }
                    } else {
                        state.pwms[i].configure(pwm_config);
                    }

                    pwm_index = Some(i);
                    break;
                }

                let pwm_index = pwm_index.ok_or_else(|| {
                    ExecuteError::ErrorCode(PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED)
                })?;

                // Per datasheet, configure PWM pin in GPIO peripheral before handing it over.
                {
                    // TODO: Ensure that we don't do any reseting of GPIO settings when this is dropped.
                    let mut pin = state.gpio.pin(IndexedPin { index: req.pin() });
                    pin.set_direction(PinDirection::Output);
                    pin.write(PinLevel::Low); // TODO: Check the default_value?
                    pin.set_high_drive(req.high_drive());
                }

                let channel = match state.pwms[pwm_index].connect(IndexedPin { index: req.pin() }) {
                    Some(v) => v,
                    None => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                        ));
                    }
                };

                let default_value = req.default_value() as u16;
                let inverted = req.inverted();
                state.pwms[pwm_index].set_value(channel, default_value, inverted);

                let mut last_active = None;
                let mut timeout = None;
                if req.timeout_millis() > 0 {
                    timeout = Some(req.timeout_millis());
                    last_active = Some(self.clock.now());
                }

                state.entries[peripheral_idx] = PeripheralEntry::PWM {
                    index: pwm_index,
                    channel,
                    default_value,
                    inverted,
                    timeout_millis: timeout,
                    last_active,
                };

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::ConfigureUart(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;
                self.check_valid_pin(req.tx_pin())?;
                self.check_valid_pin(req.rx_pin())?;

                if state.uarte.is_empty() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                // TODO: Check the pin is unused.

                // TODO: Mark the pin as in use.

                let tx_pin = IndexedPin { index: req.tx_pin() };
                let rx_pin = IndexedPin { index: req.rx_pin() };
                let inst = state.uarte.pop().unwrap();

                // TODO: Validate the baud_rate is ok.

                state.entries[peripheral_idx] = PeripheralEntry::UARTE(
                    UARTE::new(inst, tx_pin, rx_pin, req.baud_rate() as usize)
                );

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::ConfigureStepper(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;

                self.check_valid_pin(req.step_pin())?;
                self.check_valid_pin(req.dir_pin())?;

                let step_pin = state.gpio.pin(IndexedPin { index: req.step_pin() });
                let dir_pin = state.gpio.pin(IndexedPin { index: req.dir_pin() });

                let controller = StepperMotorController::new(
                    step_pin,
                    dir_pin,
                    req.pulse_width(),
                    &mut state.ppi,
                    &mut state.gpiote,
                    &self.timer,
                ).ok_or_else(|| ExecuteError::ErrorCode(
                    PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                ))?;

                let i = lock!(state <= self.timer_controller.state.lock(), {
                    let i = state.entries.len();
                    state.entries.push(TimerControllerEntry::Stepper(controller));
                    i
                });

                state.entries[peripheral_idx] = PeripheralEntry::Stepper(i);

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::ConfigureAdc(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;
                self.check_valid_pin(req.pin())?;
                self.check_valid_pin(req.negative_pin())?;

                // TODO: Explicitly set the pins as inputs?
                let pin = IndexedPin { index: req.pin() };
                let mut negative_pin = None;
                if req.has_negative_pin() {
                    negative_pin = Some(IndexedPin { index: req.negative_pin() });
                }

                let adc = state.adc.as_mut().unwrap();

                let config = adc.create_channel_config(pin, negative_pin, req)
                    .ok_or_else(||
                        ExecuteError::ErrorCode(PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED)
                    )?;

                response.set_adc_format(config.format());
                
                state.entries[peripheral_idx] = PeripheralEntry::ADC(config);

                if !ADCSamplePeripheralThread::is_running() {
                    ADCSamplePeripheralThread::start(self);
                }

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::AllocateBuffer(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;

                let buffer = Buffer::new(req.size() as usize);

                state.entries[peripheral_idx] = PeripheralEntry::Buffer(buffer);

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::ConfigureNeopixel(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;
                self.check_valid_pin(req.pin())?;

                if state.spim.is_empty() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ));
                }

                let pin = state.gpio.pin(IndexedPin { index: req.pin() });
                let spi = state.spim.pop().unwrap();

                let inst = Neopixel::new(spi, pin, req.inverted());
                
                let buf = NeopixelDataBuffer::new(
                   BoxedSlice::<u8>::new_zeroed(NeopixelDataBuffer::<[u8; 1]>::size_for(req.num_bytes() as usize)),
                   req.inverted()
                );

                state.entries[peripheral_idx] = PeripheralEntry::Neopixel(
                    NeopixelPeripheral::new(inst, buf)
                );

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::ConfigureSpi(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;
                self.check_valid_pin(req.mosi_pin());
                self.check_valid_pin(req.miso_pin());
                self.check_valid_pin(req.cs_pin());
                self.check_valid_pin(req.sclk_pin());

                if state.spim.is_empty() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ));
                }

                let mode = match req.mode() {
                    0 => SPIMode::Mode0,
                    1 => SPIMode::Mode1,
                    2 => SPIMode::Mode2,
                    3 => SPIMode::Mode3,
                    _ => return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::UNSUPPORTED_CONFIG,
                    ))
                };

                let mut spi = None;
                for i in 0..state.spim.len() {
                    let good = {
                        if req.timed() {
                            state.spim[i].all_features_supported()
                        } else {
                            // TODO: Allow using this SPI but make it the last option.
                            !state.spim[i].all_features_supported()
                        }
                    };
                    
                    if good {
                        spi = state.spim.swap_remove(i);
                        break;
                    }
                }

                let spi = spi.unwrap();

                // TODO: Need validation of the frequency used.
                let inst = SPIHost::new(
                    spi,
                    req.frequency() as usize,
                    Some(IndexedPin { index: req.mosi_pin() }),
                    Some(IndexedPin { index: req.miso_pin() }),
                    Some(IndexedPin { index: req.sclk_pin() }),
                    Some(state.gpio.pin(IndexedPin { index: req.cs_pin() })),
                    mode
                );

                if req.timed() {
                    let c = SPITimerController::new(inst, &mut state.ppi, &self.timer).unwrap();

                    let i = lock!(state <= self.timer_controller.state.lock(), {
                        let i = state.entries.len();
                        state.entries.push(TimerControllerEntry::SPITimer(c));
                        i
                    });
                    
                    state.entries[peripheral_idx] = PeripheralEntry::SPITimer(i);
                } else {
                    state.entries[peripheral_idx] = PeripheralEntry::SPI(inst);
                }

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::ConfigureI2c(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;
                self.check_valid_pin(req.sda_pin());
                self.check_valid_pin(req.scl_pin());

                if state.twim0.is_none() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ));
                }

                // TODO: Pick a better error.
                let config = TWIM::configure(req.frequency() as usize)
                    .ok_or_else(|| ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ))?;

                let twim0 = state.twim0.take().unwrap();

                let inst = TWIM::new(
                    twim0,
                    IndexedPin { index: req.scl_pin() },
                    IndexedPin { index: req.sda_pin() },
                    config,
                );

                state.entries[peripheral_idx] = PeripheralEntry::I2C(inst);

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::ConfigureRadio(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;

                if state.radio.is_none() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                let radio = state.radio.take().unwrap();

                // TODO: There is a risk of losing the radio instance if this fails.
                let entry = RadioEntry::create(
                    radio,
                    &self.timer,
                    &mut state.ppi
                ).ok_or_else(|| ExecuteError::ErrorCode(
                    PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                ))?;

                state.entries[peripheral_idx] = PeripheralEntry::Radio(entry);

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::FinalizeConfig(_) => {
                if state.config_finalized {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::PERIPHERAL_ALREADY_CONFIGURED,
                    ));
                }

                for pwm in &mut state.pwms {
                    if pwm.has_connected_pins() {
                        pwm.start();
                    }
                }

                if state.usb_sof_tracker.is_none() {
                    let usbd = unsafe { peripherals::raw::usbd::USBD::new() };
                    
                    state.usb_sof_tracker = Some(
                        TimedEvent::create(
                            &usbd.events_sof,
                            &self.timer,
                            &mut state.ppi,
                        ).ok_or_else(|| ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                        ))?
                    );
                }

                state.config_finalized = true;

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::UnconfigureAll(_) => {
                if state.adc.is_none() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                for pwm in &mut state.pwms {
                    if pwm.started() {
                        pwm.stop();
                    }
                }

                // NOTE: There is a risk that these won't be able to immediately stop
                // the threads if they are actively being polled but that shouldn't ever
                // happen since this code should run in a lower priority interrupt that
                // doesn't interrupt these threads.
                InterruptPeripheralThread::stop();

                // NOTE: Only save to stop this if the ADC is still in the state
                ADCSamplePeripheralThread::stop();

                // TODO: This will forget requests from the current USB reset session
                // which is probably not a good thing.
                state.adc_request_queue.clear();

                for entry in state.entries.iter_mut() {
                    let mut e = PeripheralEntry::Unconfigured;
                    core::mem::swap(&mut e, entry);

                    match e {
                        PeripheralEntry::Unconfigured => {}
                        PeripheralEntry::Borrowed => {
                            // TODO: Stuff via GPIO interrupts need to be cancelled.

                            return Err(ExecuteError::ErrorCode(
                                PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                            ));
                        }
                        PeripheralEntry::PWM { index, channel, .. } => {
                            state.pwms[index].disconnect(channel);
                        }
                        PeripheralEntry::GPIO(mut entry) => {
                            entry.pin.reset();

                            if let Some(tach) = entry.tachometer.take() {
                                state.timers.push(tach.timer);
                            }
                        }
                        PeripheralEntry::UARTE(inst) => {
                            // Pins get disconnected on drop
                            state.uarte.push(inst.into_inner());
                        }
                        PeripheralEntry::Stepper(_) | PeripheralEntry::SPITimer(_) => {
                            // Handled in the timer controller checks below.
                        } 

                        PeripheralEntry::Neopixel(inst) => {
                            state.spim.push(inst.into_inner().into_inner());
                        }
                        PeripheralEntry::SPI(inst) => {
                            state.spim.push(inst.into_inner());
                        }
                        PeripheralEntry::Radio(inst) => {
                            state.radio = Some(inst.into_inner());
                        }
                        PeripheralEntry::ADC(_) => {}
                        PeripheralEntry::Buffer(_) => {}
                        PeripheralEntry::I2C(inst) => {
                            state.twim0 = Some(inst.into_inner());
                        }
                    }
                }

                lock!(timer_state <= self.timer_controller.state.lock(), {
                    while let Some(entry) = timer_state.entries.pop() {
                        match entry {
                            TimerControllerEntry::Stepper(controller) => {},
                            TimerControllerEntry::SPITimer(controller) => {
                                // NOTE: If the controller is active, we will be missing a
                                // buffer entry so the request should fail above.
                                state.spim.push(controller.into_inner().into_inner());
                            },
                        }
                    }

                    Ok(())
                })?;

                state.config_finalized = false;

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::SetPwm(req) => {
                self.check_fully_configured(state)?;

                let (index, channel, inverted) = match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::PWM {
                        index,
                        channel,
                        inverted,
                        last_active,
                        timeout_millis,
                        ..
                    } => {
                        if timeout_millis.is_some() {
                            *last_active = Some(self.clock.now());
                        }

                        (*index, *channel, *inverted)
                    }
                    _ => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                };

                // TODO: Implement the inverted stuff.
                state.pwms[index].set_value(channel, req.value() as u16, inverted);

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::SetGpioLevel(req) => {
                self.check_fully_configured(state)?;

                let entry = get_peripheral_mut!(state, peripheral_idx, GPIO);

                entry.pin.write_bool(req.high());

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::GetGpioLevel(_) => {
                self.check_fully_configured(state)?;

                let entry = get_peripheral_mut!(state, peripheral_idx, GPIO);

                // TODO: Check configured as an input.

                // NOTE: If the response is low, then the entire response can
                // be batch ack'ed so is cheap to return.
                if entry.pin.read() == PinLevel::High {
                    response.set_uint_val(1 as u32);
                }

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::PollGpioInterrupt(_) => {
                self.check_fully_configured(state)?;

                let entry = get_peripheral_mut!(state, peripheral_idx, GPIO);

                // Currently only allowed to have one request polling each pin at a time.
                if entry.pending_interrupt_sequence.is_some() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                match entry.interrupt_polarity {
                    ConfigureGPIO_InterruptPolarity::DISABLED => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                    ConfigureGPIO_InterruptPolarity::HIGH_LEVEL => {
                        entry.pin.set_sense(Some(PinLevel::High));

                        if entry.pin.read() == PinLevel::High {
                            entry.pin.set_sense(None);
                            return Ok(OkResponse);
                        }
                    }
                    ConfigureGPIO_InterruptPolarity::LOW_LEVEL => {
                        entry.pin.set_sense(Some(PinLevel::Low));

                        if entry.pin.read() == PinLevel::Low {
                            entry.pin.set_sense(None);
                            return Ok(OkResponse);
                        }
                    }

                    ConfigureGPIO_InterruptPolarity::RISING_EDGE |
                    ConfigureGPIO_InterruptPolarity::FALLING_EDGE => {
                        // TODO: Eventually support these again.
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }          
                }

                entry.pending_interrupt_sequence = Some(request.request_sequence());

                if !InterruptPeripheralThread::is_running() {
                    InterruptPeripheralThread::start(self);
                }

                Err(ExecuteError::AsyncReply)
            }
            PeripheralRequestCommandCase::Cancel(_) => {
                self.check_fully_configured(state)?;

                match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::GPIO(entry) => {
                        if entry.pending_interrupt_sequence == Some(request.request_sequence()) {
                            entry.pending_interrupt_sequence = None;
                            entry.pin.set_sense(None);

                            return Err(ExecuteError::ErrorCode(
                                PeripheralResponse_ErrorCode::CANCELLED,
                            ));
                        }
                    }
                    // Other peripherals don't implement cancellation.
                    _ => {}
                }


                Err(ExecuteError::AsyncReply)
            }
            PeripheralRequestCommandCase::ReadTachometer(_) => {
                self.check_fully_configured(state)?;

                if TachometerPeripheralThread::is_running() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                // TODO: Veriffy no existing interrupt since we will end up deleting it.

                check_peripheral_type!(state, peripheral_idx, GPIO);

                let entry = borrow_peripheral!(state, peripheral_idx, GPIO);

                TachometerPeripheralThread::start(
                    self,
                    peripheral_idx,
                    request.request_sequence(),
                    entry,
                );

                // To be returned asyncronously.
                Err(ExecuteError::AsyncReply)
            }
            PeripheralRequestCommandCase::StartTachometer(_) => {
                self.check_fully_configured(state)?;

                let gpio = get_peripheral_mut!(state, peripheral_idx, GPIO);
                
                if gpio.tachometer.is_some() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }
                
                let interrupt = state.gpiote.new_interrupt_channel(&gpio.pin, GPIOInterruptPolarity::FallingEdge)
                    .ok_or_else(|| ExecuteError::ErrorCode(PeripheralResponse_ErrorCode::RESOURCE_BUSY))?;

                let mut timer = state.timers.pop()
                    .ok_or_else(|| ExecuteError::ErrorCode(PeripheralResponse_ErrorCode::RESOURCE_BUSY))?;
                timer.mode.write_counter();
                timer.bitmode.write_32bit();
                timer.tasks_clear.write_trigger();
                // TODO: Eventually ensure the timer is always stopped.
                timer.tasks_start.write_trigger();

                let mut ppi = state.ppi.new_channel(
                    interrupt.in_event(),
                    &mut timer.tasks_count,
                ).ok_or_else(|| ExecuteError::ErrorCode(PeripheralResponse_ErrorCode::RESOURCE_BUSY))?;

                ppi.enable();

                let time = self.timer.capture()
                    .ok_or_else(|| ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ))?;
                
                response.set_uint_val(time);

                gpio.tachometer = Some(GPIOTachometerState {
                    interrupt,
                    timer,
                    ppi,
                });

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::EndTachometer(_) => {
                self.check_fully_configured(state)?;

                let gpio = get_peripheral_mut!(state, peripheral_idx, GPIO);
                
                let mut tach = gpio.tachometer.take()
                    .ok_or_else(|| ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ))?;

                let final_count = {
                    tach.timer.tasks_capture[0].write_trigger();
                    tach.timer.cc[0].read()
                };

                state.timers.push(tach.timer);


                let time = self.timer.capture()
                    .ok_or_else(|| ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ))?;
                
                response.set_time(time);

                response.set_uint_val(final_count);

                Ok(OkResponse)

            }
            PeripheralRequestCommandCase::MeasureMcuTemperature(_) => {
                self.check_fully_configured(state)?;

                if TemperaturePeripheralThread::is_running() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                let temp = state.temp.take().ok_or_else(|| {
                    ExecuteError::ErrorCode(PeripheralResponse_ErrorCode::RESOURCE_BUSY)
                })?;

                TemperaturePeripheralThread::start(self, request.request_sequence(), temp);

                Err(ExecuteError::AsyncReply)
            }

            PeripheralRequestCommandCase::UartTransmit(req) => {
                self.check_fully_configured(state)?;
                
                if UartTransmitPeripheralThread::is_running() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                check_peripheral_type!(state, peripheral_idx, UARTE);

                let inst = borrow_peripheral!(state, peripheral_idx, UARTE);

                let mut read_request = None;
                if req.has_rx_after_tx() {
                    read_request = Some(req.rx_after_tx().clone());
                }

                UartTransmitPeripheralThread::start(
                    self,
                    peripheral_idx,
                    request.request_sequence(),
                    inst,
                    req.data().into(),
                    read_request
                );

                Err(ExecuteError::AsyncReply)
            }
            PeripheralRequestCommandCase::SpiTransfer(req) => {
                self.check_fully_configured(state)?;

                if req.transfer_count() != 0 {
                    let entry_idx = *get_peripheral_mut!(state, peripheral_idx, SPITimer);
                    
                    let buffer = borrow_peripheral!(state, req.read_buffer() as usize, Buffer);

                    return lock!(state <= self.timer_controller.state.lock(), {
                        let spi = match &mut state.entries[entry_idx] {
                            TimerControllerEntry::SPITimer(v) => v,
                            _ => panic!()
                        };

                        if !spi.enqueue_request(request.request_sequence() as u8, &req, buffer) {
                            return Err(ExecuteError::ErrorCode(
                                PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                            ));
                        }

                        spi.tick();

                        Err(ExecuteError::AsyncReply)
                    });
                }

                if SPIPeripheralThread::is_running() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }
                
                check_peripheral_type!(state, peripheral_idx, SPI);

                let inst = borrow_peripheral!(state, peripheral_idx, SPI);

                SPIPeripheralThread::start(
                    self,
                    peripheral_idx,
                    request.request_sequence(),
                    inst,
                    req.data().into()
                );

                Err(ExecuteError::AsyncReply)
            }
            PeripheralRequestCommandCase::GetStackPointer(_) => {
                let mut buf = [0u8; 4];
                unsafe { core::ptr::read_volatile::<u8>(buf.as_ptr()) };

                response.set_uint_val(unsafe { core::mem::transmute::<_, u32>(buf.as_ptr()) });

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::Sleep(_) => {
                if SleepPeripheralThread::is_running() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                SleepPeripheralThread::start(self, request.request_sequence());
                Err(ExecuteError::AsyncReply)
            }
            PeripheralRequestCommandCase::Noop(_) => Ok(OkResponse),
            PeripheralRequestCommandCase::Info(_) => todo!(),

            PeripheralRequestCommandCase::EnqueueStepperMotion(req) => {
                self.check_fully_configured(state)?;

                let stepper_idx = *get_peripheral_mut!(state, peripheral_idx, Stepper);

                /*
                // Optional checks if we want to be paranoid.
                let time = self.timer.capture()
                    .ok_or_else(|| ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ))?;

                let delta_time = cnc::time_remaining_u32(req.next_step_time(), time);
                if delta_time < 30 || delta_time > 4*16_000_000 {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::TIMEOUT,
                    ));
                }
                */

                lock!(state <= self.timer_controller.state.lock(), {
                    let stepper = match &mut state.entries[stepper_idx] {
                        TimerControllerEntry::Stepper(s) => s,
                        _ => {
                            return Err(ExecuteError::ErrorCode(
                                PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                            ));
                        }
                    };

                    if !stepper.enqueue_motion(req) {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                        ));
                    }

                    stepper.tick();

                    Ok(OkResponse)
                })
            }
            PeripheralRequestCommandCase::ClearStepperQueue(_) => {
                let stepper_idx = *get_peripheral_mut!(state, peripheral_idx, Stepper);

                lock!(state <= self.timer_controller.state.lock(), {
                    let stepper = match &mut state.entries[stepper_idx] {
                        TimerControllerEntry::Stepper(s) => s,
                        _ => {
                            return Err(ExecuteError::ErrorCode(
                                PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                            ));
                        }
                    };

                    let time = self.timer.capture()
                        .ok_or_else(|| ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                        ))?;

                    stepper.clear_motions(time);
                    stepper.tick();
                    Ok(OkResponse)

                })


            }
            PeripheralRequestCommandCase::ResetStepperMotor(_) => {
                let stepper_idx = *get_peripheral_mut!(state, peripheral_idx, Stepper);

                lock!(state <= self.timer_controller.state.lock(), {
                    let stepper = match &mut state.entries[stepper_idx] {
                        TimerControllerEntry::Stepper(s) => s,
                        _ => {
                            return Err(ExecuteError::ErrorCode(
                                PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                            ));
                        }
                    };

                    stepper.reset();
                    Ok(OkResponse)
                })
                
                // TODO: THis should probably reset an erorr if not stopped or there are entries in the queue.
                // let stepper = get_peripheral_mut!(state, peripheral_idx, Stepper);

            }

            PeripheralRequestCommandCase::GetStepperMotorStatus(_) => {
                self.check_fully_configured(state)?;

                let stepper_idx = *get_peripheral_mut!(state, peripheral_idx, Stepper);

                lock!(state <= self.timer_controller.state.lock(), {
                    let stepper = match &mut state.entries[stepper_idx] {
                        TimerControllerEntry::Stepper(s) => s,
                        _ => {
                            return Err(ExecuteError::ErrorCode(
                                PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                            ));
                        }
                    };

                    response.set_stepper_status(stepper.status());

                    
                    Ok(OkResponse)
                })
            }
            PeripheralRequestCommandCase::GetClockTime(_) => {
                let time = self.timer.capture()
                    .ok_or_else(|| ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ))?;
                
                response.set_uint_val(time);
                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::GetUsbSofTime(_) => {
                /*
                This has to deal with race conditions related to a new start_of_frame packet
                being received while this is executing. An ideal solution would be to capture
                the values in an interrupt waiting on EVENTS_SOF but that would require more
                RTT delay or waking up every 1ms which is likely going to be more expensive than
                just handlign the race conditions in here since the host polls the time much less
                than 1000 times a second.

                Timing:
                - FRAMECNTR is updated
                - Then EVENTS_SOF is triggered
                - Then we capture the frame time.

                There is a small chance we get a misaligned time and frame counter.
                */

                self.check_fully_configured(state)?;

                use core::arch::asm;
                use crate::common::register::RegisterRead;

                let usbd = unsafe { peripherals::raw::usbd::USBD::new() };

                let frame_counter = usbd.framecntr.read();

                // Just in case the frame counter was just updated, wait at least 2 PPI cycles to ensure that
                // the counter is also updated.
                //
                // TODO: For whatever reason, this still isn't sufficient to ensure
                // that there aren't any issues.
                unsafe {
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");

                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");

                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                    asm!("nop");
                }

                let sof_time = state.usb_sof_tracker.as_ref().unwrap().last_time();

                // Check for the rare case that we are on the edge of a frame so can't 
                // easily correlate the time and frame counter values.
                let frame_counter2 = usbd.framecntr.read();
                if frame_counter2 != frame_counter {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::ABORTED,
                    ));
                }

                let now = self.timer.capture()
                    .ok_or_else(|| ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ))?;

                response.usb_sof_mut().set_frame_start_time(sof_time);
                response.usb_sof_mut().set_frame_counter(frame_counter);
                response.set_time(now);
                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::GetIdleCounter(_) => {
                let v = crate::idle::idle_counter_value();
                response.set_uint_val(v);
                Ok(OkResponse)
            }


            PeripheralRequestCommandCase::SampleAdc(req) => {
                self.check_fully_configured(state)?;

                // NOTE: The validity of the peripheral and buffer indexes
                // is checked during request execution (after enqueuing).
                let inner_request = ADCRequest {
                    request_sequence: request.request_sequence() as u8,
                    typ: if req.has_buffer() {
                        ADCRequestType::WindowSample {
                            peripheral_index: peripheral_idx as u8,
                            buffer_index: req.buffer() as u8,
                        }
                    } else {
                        ADCRequestType::SingleSample {
                            peripheral_index: peripheral_idx as u8,
                        }
                    }
                };

                let was_empty = state.adc_request_queue.is_empty();

                if !state.adc_request_queue.enqueue(inner_request) {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ));
                }

                if was_empty {
                    self.adc_request_queue_filled.try_send(());
                }

                Err(ExecuteError::AsyncReply)
            }
            PeripheralRequestCommandCase::CalibrateAdc(_) => {
                self.check_fully_configured(state)?;

                // TODO: Dedup with SampleAdc.

                let inner_request = ADCRequest {
                    request_sequence: request.request_sequence() as u8,
                    typ: ADCRequestType::Calibrate
                };

                let was_empty = state.adc_request_queue.is_empty();

                if !state.adc_request_queue.enqueue(inner_request) {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ));
                }

                if was_empty {
                    self.adc_request_queue_filled.try_send(());
                }

                Err(ExecuteError::AsyncReply)
            }

            PeripheralRequestCommandCase::ReadBuffer(_) => {
                self.check_fully_configured(state)?;

                let buf = get_peripheral_mut!(state, peripheral_idx, Buffer);

                buf.read(response);
                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::NeopixelTransfer(req) => {
                self.check_fully_configured(state)?;

                check_peripheral_type!(state, peripheral_idx, Neopixel);
                
                let inst = get_peripheral_mut!(state, peripheral_idx, Neopixel);

                // TODO: Propagate errors.
                inst.write(req.index() as usize, req.data());

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::NeopixelShow(req) => {
                self.check_fully_configured(state)?;

                check_peripheral_type!(state, peripheral_idx, Neopixel);

                if NeopixelPeripheralThread::is_running() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }
                
                let inst = borrow_peripheral!(state, peripheral_idx, Neopixel);

                NeopixelPeripheralThread::start(
                    self,
                    peripheral_idx,
                    request.request_sequence(),
                    inst
                );

                Err(ExecuteError::AsyncReply)
            }
            PeripheralRequestCommandCase::I2cTransfer(req) => {
                self.check_fully_configured(state)?;

                check_peripheral_type!(state, peripheral_idx, I2C);

                if I2CPeripheralThread::is_running() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                let inst = borrow_peripheral!(state, peripheral_idx, I2C);

                I2CPeripheralThread::start(
                    self,
                    peripheral_idx,
                    request.request_sequence(),
                    inst,
                    req.as_ref().clone()
                );

                Err(ExecuteError::AsyncReply)
            }
        }
    }

    /// Helper to check that we are in a state that can accept configuration of
    /// a peripheral entry. Always call this as the first check in
    /// configuration commands.
    fn check_entry_not_configured(
        &self,
        state: &mut PeripheralsControllerState,
        peripheral_idx: usize,
    ) -> Result<(), ExecuteError> {
        if state.config_finalized {
            return Err(ExecuteError::ErrorCode(
                PeripheralResponse_ErrorCode::PERIPHERAL_ALREADY_CONFIGURED,
            ));
        }

        match state.entries[peripheral_idx] {
            PeripheralEntry::Unconfigured => Ok(()),
            _ => Err(ExecuteError::ErrorCode(
                PeripheralResponse_ErrorCode::PERIPHERAL_ALREADY_CONFIGURED,
            )),
        }
    }

    /// Always call this as the first thing for peripheral accessing commands.
    fn check_fully_configured(&self, state: &mut PeripheralsControllerState) -> Result<(), ExecuteError> {
        if !state.config_finalized {
            return Err(ExecuteError::ErrorCode(
                PeripheralResponse_ErrorCode::CONFIG_NOT_FINALIZED,
            ));
        }

        Ok(())
    }

    fn check_valid_pin(&self, pin: u32) -> Result<(), ExecuteError> {
        if pin as usize >= NUM_PINS {
            return Err(ExecuteError::ErrorCode(
                PeripheralResponse_ErrorCode::PIN_OUT_OF_RANGE,
            ));
        }

        Ok(())
    }

    pub(super) fn write_response(&self, state: &mut PeripheralsControllerState, res: &PeripheralResponse) {
        // This sequence is just used for locally triggered init commands.
        if res.request_sequence() == 0 {
            return;
        }

        let has_response = match res.response_case() {
            PeripheralResponseResponseCase::NOT_SET => false,
            _ => true
        };

        if res.error_code() == PeripheralResponse_ErrorCode::NO_ERROR && !has_response {
            if let Some(last_batch_ack) = &mut state.batch_ack {
                if last_batch_ack.first_sequence + last_batch_ack.num_requests + 1 == res.request_sequence() {
                    last_batch_ack.num_requests += 1;
                    return;
                }

                let batch_ack = state.batch_ack.take().unwrap();
                self.write_batch_ack(state, batch_ack);
            }

            state.batch_ack = Some(BatchAck {
                first_sequence: res.request_sequence(),
                num_requests: 0
            });
            self.response_buffer_readable.try_send(());
            return;
        }

        self.write_response_raw(state, res);
    }

    fn write_batch_ack(&self, state: &mut PeripheralsControllerState, batch_ack: BatchAck) {
        let mut res = PeripheralResponse::default();
        res.set_request_sequence(batch_ack.first_sequence);
        if batch_ack.num_requests > 0 {
            res.set_ack_next_n(batch_ack.num_requests);
        }
        self.write_response_raw(state, &res);
    }

    fn write_response_raw(&self, state: &mut PeripheralsControllerState, res: &PeripheralResponse) {
        // Writes are paused until we report the overflow.
        if state.response_buffer_overflowed {
            return;
        }

        let mut c = state.response_buffer.checkpoint();
        c.push(0);
        res.serialize_to(&protobuf::SerializeOptions::default(), &mut c).unwrap();

        if c.overflowed() {
            state.response_buffer_overflowed = true;
        } else {
            // TODO: Check for responses above 256 bytes in length
            c[0] = (c.len() - 1) as u8;
        }

        self.response_buffer_readable.try_send(());
    }
}
