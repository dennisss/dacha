use core::arch::asm;

use common::register::{RegisterRead, RegisterWrite};
use peripherals::raw::pwm0::prescaler::PRESCALER_FIELD;
use peripherals::raw::pwm0::r#loop::LOOP_VALUE;
use peripherals::raw::pwm0::seq::enddelay::ENDDELAY_VALUE;
use peripherals::raw::pwm0::seq::refresh::REFRESH_VALUE;
use peripherals::raw::pwm0::PWM0;

use crate::{
    events::flush_events_clear,
    pins::{connect_pin, disconnect_pin, PeripheralPin},
    usb::aligned::Aligned,
};

const MAX_COUNTERTOP: u32 = (1 << 15) - 1;
const MIN_COUNTERTOP: u32 = 3;

const PRESCALAR_FREQUENCIES: &'static [(u32, PRESCALER_FIELD)] = &[
    (16_000_000, PRESCALER_FIELD::DIV_1),
    (8_000_000, PRESCALER_FIELD::DIV_2),
    (4_000_000, PRESCALER_FIELD::DIV_4),
    (2_000_000, PRESCALER_FIELD::DIV_8),
    (1_000_000, PRESCALER_FIELD::DIV_16),
    (500_000, PRESCALER_FIELD::DIV_32),
    (250_000, PRESCALER_FIELD::DIV_64),
    (125_000, PRESCALER_FIELD::DIV_128),
];

/// Implementation of basic continous PWM wave generation (repeating square wave
/// with fixed frequency / duty cycle until changed by the user).
///
/// Each 'PWM' instance corresponds to one hardware nRF52 PWM[0-3] peripheral so
/// can control up to 4 individually controllable channels at the same
/// frequency.
///
/// Usage:
/// - Call PWM::new()
/// - Call PWM::configure()
///   - NOTE: This should only be
/// - Call PWM::connect() on all output pins.
///   - Note: This can be done after starting but is not recommended.
/// - Call PWM::set_value() to configure initial values.
/// - Call PWM::start() to enable the peripheral and start PWM generation.
/// - Call PWM::set_value to dynamically update duty cycles over time...
/// - Call PWM::stop()
/// - At this point, all pins are still connected but
///
/// Internal implementation:
/// - At the start of each PWM period, we have the counter start and 0 and just
///   count up until we hit the per-channel compare value and eventually reset
///   to 0 for the next period.
/// - Just 4 compare values are stored in each sequence (1 for each channel).
/// - We initially start playing SEQ[0] and use looping to then play SEQ[1] and
///   so on.
/// - Both SEQ[0] and SEQ[1] point to the same compare values buffer.
/// - When the looping is done, we use the LOOPSDONE_SEQSTART0 to automatically
///   restart.
///
/// Note that an alternative implementation would be to just use SEQ[0] and
/// LOOP.CNT=0. Once the sequence is done, it would infinitely repeat the last
/// value, but this has the disadvantage of requiring us to call SEQSTART
/// whenever we want to update the duty cycles which may potentially glitch and
/// not complete full PWM periods is the duty cycles are updated too quickly.
pub struct PWM {
    periph: PWM0,
    /// NOTE: This must be stored in RAM since it is EasyDMA referenced.
    sequence_data: Aligned<[u16; 4], u32>,
}

/// Hardware configuration internally decided to run at the requested frequency.
#[derive(Clone, PartialEq, Eq)]
pub struct PWMConfig {
    prescaler: PRESCALER_FIELD,
    countertop: u32,
}

impl PWMConfig {
    pub fn from_frequency(frequency: u32) -> Option<Self> {
        if frequency == 0 {
            return None;
        }

        let mut best_countertop = 0;
        let mut best_prescalar = PRESCALER_FIELD::DIV_1;

        for (prescalar_freq, prescaler_field) in PRESCALAR_FREQUENCIES.iter().cloned() {
            // TODO: Round this?
            let countertop = prescalar_freq / frequency;

            if countertop > MAX_COUNTERTOP {
                // Infeasible.
                continue;
            }

            best_countertop = countertop;
            best_prescalar = prescaler_field;
            break;
        }

        if best_countertop < MIN_COUNTERTOP {
            return None;
        }

        Some(Self {
            prescaler: best_prescalar,
            countertop: best_countertop,
        })
    }
}

impl PWM {
    pub fn new(mut periph: PWM0) -> Self {
        Self {
            periph,
            sequence_data: Aligned::new([0; 4]),
        }
    }

    pub fn config(&self) -> PWMConfig {
        PWMConfig {
            prescaler: self.periph.prescaler.read(),
            countertop: self.periph.countertop.read(),
        }
    }

    /// Configures the PWM module with a specific configuration.
    ///
    /// The module may be reconfigured when stopped.
    ///
    /// TODO: self must be pinned.
    pub fn configure(&mut self, config: PWMConfig) {
        // Count up.
        self.periph
            .mode
            .write_with(|v| v.set_updown_with(|v| v.set_up()));

        // The actual value here doesn't matter since the short will make this infinite.
        self.periph.r#loop.write(LOOP_VALUE::from_raw(1024));

        self.periph
            .shorts
            .write_with(|v| v.set_loopsdone_seqstart0_with(|v| v.set_enabled()));

        self.periph.decoder.write_with(|v| {
            v.set_load_with(|v| v.set_individual())
                .set_mode_with(|v| v.set_refreshcount())
        });

        self.periph.prescaler.write(config.prescaler);
        self.periph.countertop.write(config.countertop);

        for i in 0..self.periph.seq.len() {
            self.periph.seq[i]
                .ptr
                .write(unsafe { core::mem::transmute(self.sequence_data.as_ptr()) });
            self.periph.seq[i]
                .cnt
                .write(peripherals::raw::pwm0::seq::cnt::CNT_FIELD::Unknown(
                    self.sequence_data.len() as u32,
                ));

            self.periph.seq[i].refresh.write(REFRESH_VALUE::from_raw(0));
            self.periph.seq[i]
                .enddelay
                .write(ENDDELAY_VALUE::from_raw(0));
        }
    }

    /// NOTE: This assumes that the PWM module isn't already running.
    pub fn start(&mut self) {
        self.periph.enable.write_enabled();

        // loop {
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
            asm!("nop");
        }
        // }

        // TODO: Unset the seqstart event.

        self.periph.tasks_seqstart[0].write_trigger();
    }

    /// NOTE: This will return true immediately after calling start() and will
    /// not block for the first PWM period to start.
    pub fn started(&self) -> bool {
        self.periph.enable.read().is_enabled()
    }

    /// NOTE: This blocks for PWM generation to fully stop.
    pub fn stop(&mut self) {
        // TODO: Return if not running.

        self.periph.events_stopped.write_notgenerated();
        flush_events_clear();

        self.periph.tasks_stop.write_trigger();

        while self.periph.events_stopped.read().is_notgenerated() {
            unsafe { asm!("nop") };
        }

        // TODO: Figure out if this is sufficient or if we actually need to run the stop
        // task to fully reset the peripheral back to an uninitialized state.
        self.periph.enable.write_disabled();
    }

    /// Connects a new pin to the output of the next free channel in the
    /// peripheral. (assumes the pin isn't connected to anything yet)
    ///
    /// Returns the index of the channel (or None if all channels are occupied).
    ///
    /// TODO: It is recommended to connect all pins before enabling the PWM
    /// module.
    pub fn connect<P: PeripheralPin>(&mut self, pin: P) -> Option<usize> {
        for i in 0..self.periph.psel.out.len() {
            if self.periph.psel.out[i].read().connect().is_connected() {
                continue;
            }

            connect_pin(pin, &mut self.periph.psel.out[i]);
            return Some(i);
        }

        None
    }

    /// Returns whether or not any pins are still connected to this module.
    pub fn has_connected_pins(&self) -> bool {
        for i in 0..self.periph.psel.out.len() {
            if self.periph.psel.out[i].read().connect().is_connected() {
                return true;
            }
        }

        false
    }

    pub fn disconnect(&mut self, channel: usize) {
        disconnect_pin(&mut self.periph.psel.out[channel]);
    }

    /// Changes the duty cycle and/or polarity of a single channel.
    ///
    /// This change will be picked up when the next PWM period starts.
    ///
    /// NOTE: This can only be called after the PWM peripheral is configured
    /// (with start()).
    ///
    /// Arguments:
    /// - 'channel'
    /// - 'value': should be in the full 16-bit range and will be scaled down
    ///   based on the available hardware resolution.
    /// - 'inverted': If false, the duty cycle is the amount of time the pin is
    ///   high, else, it is the amount of time the pin is low.
    pub fn set_value(&mut self, channel: usize, value: u16, inverted: bool) {
        let value = value as u32;
        let countertop = self.periph.countertop.read() as u32;
        let effective_value = ((countertop * value) / ((1 << 16) - 1)) as u16;

        let msb = if inverted { 0 } else { 1 << 15 };

        self.sequence_data[channel] = msb | effective_value;
    }
}
