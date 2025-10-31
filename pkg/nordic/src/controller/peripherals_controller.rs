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

*/

use common::fixed::vec::FixedVec;
use common::segmented_buffer::SegmentedBuffer;
use executor::sync::AsyncMutex;
use peripherals_proto::peripherals::{
    PeripheralRequest, PeripheralRequestCommandCase, PeripheralResponse,
    PeripheralResponse_ErrorCode,
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
use peripherals::raw::uarte0::UARTE0;
use peripherals::raw::ppi::PPI;
use protobuf::{Message, StaticMessage};

use crate::gpio::*;
use crate::gpiote::GPIOTEChannels;
use crate::pins::{PeripheralPin, Port};
use crate::pwm::{PWMConfig, PWM};
use crate::rtc::{RTCInstant, RTC};
use crate::temp::Temp;
use crate::uarte::UARTE;
use crate::timer::Timer;
use crate::ppi::PPIChannels;
use crate::spi::*;
use crate::neopixel::*;

use super::neopixel::NeopixelPeripheralThread;
use super::tachometer::TachometerPeripheralThread;
use super::temp::TemperaturePeripheralThread;
use super::timeout::TimeoutPeripheralThread;
use super::uart::UartTransmitPeripheralThread;
use super::stepper::{StepperMotorController, StepperMotion, StepperPeripheralThread};

// Port 0 has 32 pins.
// Port 1 has 16 pins.
//
// TODO: Update based on MCU type.
const NUM_PINS: usize = 48;

const RESPONSE_BUFFER_SIZE: usize = 512;

const DEFAULT_ENTRY_VALUE: PeripheralEntry = PeripheralEntry::Unconfigured;

pub struct PeripheralsController {
    pub(super) clock: RTC,
    pub(super) state: AsyncMutex<State>,
}

// TODO: Unconfigure all peripherals when this is dropped.
pub(super) struct State {
    pub entries: [PeripheralEntry; 16],

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

    pub uarte: Option<UARTE0>,

    pub spim: FixedVec<SPIMx, 4>,

    pub timer: Timer,

    /// Stores PeripheralResponse protos that need to be read back by the host.
    pub response_buffer: SegmentedBuffer<[u8; RESPONSE_BUFFER_SIZE]>,
}

pub(super) enum PeripheralEntry {
    Unconfigured,

    /// A thread is currently operating on this peripheral and has exclusive
    /// access to it.
    Borrowed,

    GPIO {
        pin: GPIOPin,
        interrupt: Option<GPIOInterruptChannel>,
    },
    PWM {
        index: usize,
        channel: usize,
        inverted: bool,
        default_value: u16,
        last_active: Option<RTCInstant>,
        timeout_millis: Option<u32>,
    },

    UARTE {
        inst: UARTE,
    },

    Stepper {
        controller: StepperMotorController
    },

    Neopixel {
        inst: Neopixel
    }
}

/*
Memory required for the PWM peripheral?
- Basically nothing (I can tell if a PWM thing is enabled)
    - But I need to be able to disable it.

*/

/*


Handling async operations:
- We have a thread pool
    - We start a thread giving it a task to do.

*/

/*

SAADC:
- Configure all channels initially
    - Inputs:
        - Positive Pin
        - Negative Pin (may also want to support this being a reference voltage)
        - Desired voltage range to measure
            - Used to set the gains and set optimially.
        - Max sample rate
        - Max acquisition time.
        - Target resolution?
    - Returns:
        - Units per Volt
        - Min voltage in range
        - Max voltage in range

- After all pins are configured:
    - Start ADC thread
    - Generally simplest to just keep the sampling always running
    - Note that there will be some skew in the readings since they are sampled one after another.
- When the user requests an ADC voltage
    - Verify the ADC is still healthy.
    - Take the last sampled one.

- TODO: also need to deal with calibration (for now just do during init)

- Ideally just short the 'EVENT_END' -> 'TASKS_START'

- Read 'RESULT.AMOUNT' to see how many memory entries have already been written.

Need to wrangel all the flash memory usage.

Need likely separate space for the network config and for the static stuff.

*/

impl Default for PeripheralEntry {
    fn default() -> Self {
        Self::Unconfigured
    }
}

struct IndexedPin {
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
        timer0: TIMER0,
        ppi: PPI,
        saadc: SAADC
    ) -> Self {
        // TODO: Don't create this here. We should ban calling this outside of main().
        let mut peripherals = peripherals::raw::Peripherals::new();
        let mut gpio = GPIO::new(peripherals.p0, peripherals.p1);
        let timer = Timer::new(timer0);

        let mut entries = [DEFAULT_ENTRY_VALUE; 16];
        for i in 0..entries.len() {
            entries[i] = PeripheralEntry::Unconfigured;
        }

        // TODO: Instead init from a slice.
        let mut spim = FixedVec::new();
        spim.push(spim0.into());
        spim.push(spim1.into());
        spim.push(spim2.into());
        spim.push(spim3.into());

        Self {
            clock,
            state: AsyncMutex::new(State {
                entries,
                config_finalized: false,
                pwms: [
                    PWM::new(pwm0.into()),
                    PWM::new(pwm1.into()),
                    PWM::new(pwm2.into()),
                    PWM::new(pwm3.into()),
                ],
                uarte: Some(uarte0),
                gpio,
                gpiote: GPIOTEChannels::new(gpiote),
                ppi: PPIChannels::new(ppi),
                temp: Some(Temp::new(temp)),
                used_pins: [0; NUM_PINS / 4],
                spim,
                timer,
                response_buffer: SegmentedBuffer::new([0u8; RESPONSE_BUFFER_SIZE]),
            }),
        }
    }

    pub fn start(&'static self) {
        TimeoutPeripheralThread::start(self);
    }

    /// TODO: This requires that 'self' is pinned.
    pub async fn execute(&'static self, request: &PeripheralRequest) {
        let mut res = PeripheralResponse::default();

        lock!(state <= self.state.lock().await.unwrap(), {
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

    pub async fn read_response(&self, out: &mut [u8]) -> Option<usize> {
        lock!(state <= self.state.lock().await.unwrap(), {
            state.response_buffer.read(out)
        })
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
        state: &mut State,
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

                // TODO: Check the pin index is valid.

                if req.pin() as usize >= NUM_PINS {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::PIN_OUT_OF_RANGE,
                    ));
                }

                // TODO: Check the pin is unused.

                // TODO: Mark the pin as in use.

                let mut pin = state.gpio.pin(IndexedPin { index: req.pin() });

                if !req.is_input() {
                    pin.write(if req.default_value() { PinLevel::High } else { PinLevel::Low });
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


                let mut interrupt = None;
                
                let interrupt_polarity = match req.interrupt() {
                    ConfigureGPIO_InterruptPolarity::DISABLED => None,
                    ConfigureGPIO_InterruptPolarity::RISING_EDGE => Some(GPIOInterruptPolarity::RisingEdge),
                    ConfigureGPIO_InterruptPolarity::FALLING_EDGE => Some(GPIOInterruptPolarity::FallingEdge)
                };

                if let Some(polarity) = interrupt_polarity {
                    interrupt = Some(state.gpiote.new_interrupt_channel(
                        IndexedPin { index: req.pin() },
                        polarity
                    ));

                    // TODO: Start the background thread if not already started.
                }

                state.entries[peripheral_idx] = PeripheralEntry::GPIO { pin, interrupt };

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::ConfigurePwm(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;

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

                // TODO: Dedup this logic more.
                if req.tx_pin() as usize >= NUM_PINS {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::PIN_OUT_OF_RANGE,
                    ));
                }

                if req.rx_pin() as usize >= NUM_PINS {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::PIN_OUT_OF_RANGE,
                    ));
                }

                if state.uarte.is_none() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                // TODO: Check the pin is unused.

                // TODO: Mark the pin as in use.

                let tx_pin = IndexedPin { index: req.tx_pin() };
                let rx_pin = IndexedPin { index: req.rx_pin() };
                let inst = state.uarte.take().unwrap();

                // TODO: Validate the baud_rate is ok.

                state.entries[peripheral_idx] = PeripheralEntry::UARTE {
                    inst: UARTE::new(inst, tx_pin, rx_pin, req.baud_rate() as usize)
                };

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::ConfigureStepper(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;

                // TODO: Check the pin index is valid.

                if req.step_pin() as usize >= NUM_PINS ||
                   req.dir_pin() as usize >= NUM_PINS {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::PIN_OUT_OF_RANGE,
                    ));
                }

                let step_pin = state.gpio.pin(IndexedPin { index: req.step_pin() });
                let dir_pin = state.gpio.pin(IndexedPin { index: req.dir_pin() });

                let controller = StepperMotorController::new(
                    step_pin,
                    dir_pin,
                    &mut state.ppi,
                    &mut state.gpiote,
                    &mut state.timer,
                ).ok_or_else(|| ExecuteError::ErrorCode(
                    PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                ))?;

                
                state.entries[peripheral_idx] = PeripheralEntry::Stepper {
                    controller
                };

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::ConfigureAdc(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;

                todo!()
            }
            PeripheralRequestCommandCase::ConfigureNeopixel(req) => {
                self.check_entry_not_configured(state, peripheral_idx)?;

                if req.pin() as usize >= NUM_PINS {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::PIN_OUT_OF_RANGE,
                    ));
                }

                if state.spim.is_empty() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ));
                }

                let pin = state.gpio.pin(IndexedPin { index: req.pin() });
                let spi = state.spim.pop().unwrap();

                state.entries[peripheral_idx] = PeripheralEntry::Neopixel {
                    inst: Neopixel::new(spi, self.clock.clone(), pin)
                };

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

                state.config_finalized = true;

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::UnconfigureAll(_) => {
                for pwm in &mut state.pwms {
                    if pwm.started() {
                        pwm.stop();
                    }
                }

                StepperPeripheralThread::stop();

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
                        PeripheralEntry::GPIO { mut pin } => {
                            pin.reset();
                        }
                        PeripheralEntry::UARTE { inst } => {
                            // Pins get disconnected on drop
                            state.uarte = Some(inst.into_inner());
                        }
                        PeripheralEntry::Stepper { controller } => {} 

                        PeripheralEntry::Neopixel { inst } => {
                            state.spim.push(inst.into_inner());
                        }
                    }
                }

                // TODO: Do this automatically
                state.timer.reset();

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

                let pin = match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::GPIO { pin, .. } => pin,
                    _ => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                };

                pin.write(if req.high() {
                    PinLevel::High
                } else {
                    PinLevel::Low
                });

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::GetGpioLevel(_) => {
                self.check_fully_configured(state)?;

                let pin = match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::GPIO { pin, .. } => pin,
                    _ => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                };

                // TODO: Check configured as an input.

                if pin.read() == PinLevel::High {
                    response.set_uint_val(1 as u32);
                }

                //
                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::ReadTachometer(_) => {
                self.check_fully_configured(state)?;

                if TachometerPeripheralThread::is_running() {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_BUSY,
                    ));
                }

                // TODO: Veriffy no existing interrupt since we will end up deleting it.

                // TODO: Check that the thread isn't running.

                // TODO: Place into a 'Borrowed' state.
                match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::GPIO { .. } => {}
                    _ => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                };

                let pin = {
                    let mut e = PeripheralEntry::Borrowed;
                    core::mem::swap(&mut e, &mut state.entries[peripheral_idx]);
                    match e {
                        PeripheralEntry::GPIO { pin, .. } => pin,
                        _ => panic!(),
                    }
                };

                TachometerPeripheralThread::start(
                    self,
                    peripheral_idx,
                    request.request_sequence(),
                    pin,
                );

                // To be returned asyncronously.
                Err(ExecuteError::AsyncReply)
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

                match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::UARTE { inst } => {}
                    _ => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                };

                let inst = {
                    let mut e = PeripheralEntry::Borrowed;
                    core::mem::swap(&mut e, &mut state.entries[peripheral_idx]);
                    match e {
                        PeripheralEntry::UARTE { inst } => inst,
                        _ => panic!(),
                    }
                };

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

            PeripheralRequestCommandCase::GetStackPointer(_) => {
                let mut buf = [0u8; 4];
                unsafe { core::ptr::read_volatile::<u8>(buf.as_ptr()) };

                response.set_uint_val(unsafe { core::mem::transmute::<_, u32>(buf.as_ptr()) });

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::Sleep(_) => {
                todo!()
            }
            PeripheralRequestCommandCase::Now(_) => {
                todo!()
            }
            PeripheralRequestCommandCase::Noop(_) => Ok(OkResponse),
            PeripheralRequestCommandCase::Info(_) => todo!(),

            PeripheralRequestCommandCase::EnqueueStepperMotion(req) => {
                let motion = StepperMotion {
                    direction: req.direction(),
                    next_time: req.next_time(),
                    next_velocity: req.next_velocity(),
                    acceleration: req.acceleration(),
                    num_steps: req.num_steps() as usize
                };

                let stepper = match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::Stepper { controller } => controller,
                    _ => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                };

                if !stepper.enqueue_motion(motion) {
                    return Err(ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ));
                }

                stepper.tick();

                if !StepperPeripheralThread::is_running() {
                    StepperPeripheralThread::start(self);
                }

                Ok(OkResponse)
            }
            PeripheralRequestCommandCase::GetClockTime(_) => {
                let time = state.timer.capture()
                    .ok_or_else(|| ExecuteError::ErrorCode(
                        PeripheralResponse_ErrorCode::RESOURCE_EXHAUSTED,
                    ))?;
                
                response.set_uint_val(time);
                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::GetStepperMotorStatus(_) => {
                let stepper = match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::Stepper { controller } => controller,
                    _ => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                };

                response.set_stepper_status(stepper.status());

                Ok(OkResponse)
            }

            PeripheralRequestCommandCase::SingleAdcSample(_) => {

                todo!()
            }
            PeripheralRequestCommandCase::NeopixelTransfer(req) => {
                self.check_fully_configured(state)?;

                match &mut state.entries[peripheral_idx] {
                    PeripheralEntry::Neopixel { .. } => {}
                    _ => {
                        return Err(ExecuteError::ErrorCode(
                            PeripheralResponse_ErrorCode::INCOMPATIBLE_COMMAND,
                        ));
                    }
                };

                let inst = {
                    let mut e = PeripheralEntry::Borrowed;
                    core::mem::swap(&mut e, &mut state.entries[peripheral_idx]);
                    match e {
                        PeripheralEntry::Neopixel { inst } => inst,
                        _ => panic!(),
                    }
                };

                NeopixelPeripheralThread::start(
                    self,
                    peripheral_idx,
                    request.request_sequence(),
                    inst,
                    req.data().into()
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
        state: &mut State,
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
    fn check_fully_configured(&self, state: &mut State) -> Result<(), ExecuteError> {
        if !state.config_finalized {
            return Err(ExecuteError::ErrorCode(
                PeripheralResponse_ErrorCode::CONFIG_NOT_FINALIZED,
            ));
        }

        Ok(())
    }

    pub(super) fn write_response(&self, state: &mut State, res: &PeripheralResponse) {
        // This sequence is just used for locally triggered init commands.
        if res.request_sequence() == 0 {
            return;
        }

        // TODO: We can just directly serialize to the output buffer if we take out a
        // view?
        let mut raw_proto = common::fixed::vec::FixedVec::<u8, 256>::new();
        res.serialize_to(&protobuf::SerializeOptions::default(), &mut raw_proto).unwrap();
        state.response_buffer.write(&raw_proto);
    }
}
