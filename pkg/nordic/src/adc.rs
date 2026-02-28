use core::ops::{Deref, DerefMut};

use common::register::RegisterRead;
use common::register::RegisterWrite;
use peripherals::raw::saadc::SAADC;
use peripherals::raw::saadc::ch::config::{CONFIG_VALUE, GAIN_FIELD, REFSEL_FIELD, RESP_FIELD, RESN_FIELD};
use peripherals::raw::saadc::ch::limit::LIMIT_VALUE;
use peripherals::raw::saadc::oversample::OVERSAMPLE_FIELD;
use peripherals::raw::EventRegister;
use peripherals::raw::Interrupt;
use peripherals::raw::TaskRegister;
use peripherals::raw::AnalogPinSelect;
use peripherals::raw::InterruptState;
use peripherals::raw::timer0::{TIMER0, TIMER0_REGISTERS};
use peripherals::raw::timer1::TIMER1;
use peripherals_proto::peripherals::{ConfigureADCRequest, ConfigureADCRequest_ResistorLadder, ADCFormat};
use executor::interrupts::wait_for_irq;


use crate::rtc::RTC;
use crate::pins::{PeripheralPin, PeripheralPinHandle};
use crate::events::flush_events_clear;
use crate::ppi::*;

/*
TODO: Need to calibrate ADC with CALIBRATEOFFSET

Improvements to this:
- Calibration can be run before any channels are configured
    https://devzone.nordicsemi.com/f/nordic-q-a/30042/saadc-offset-calibration-for-each-input#:~:text=8%20years%20ago-,Hi%2C,regular%20intervals%20or%20temperature%20changes.

- It is poorly defined if CALIBRATEOFFSET data lasts across boots so it is safer to keep the ADC peripheral always enabled to avoid losing the data

- I can unconfigure a channel by deseling the PSEL pins but it may take another TAKSS_START or TASKS_SAMPLE 
    - So generally not super safe to be ever making an ADC pin into something else.


FICR CALREF??
FICR_TRIM_GLOBAL_SAADC_LINCALCOEFF_MaxIndex
*/

const VDD_VOLTAGE: f32 = 3.3;

const RESOLUTION_NUM_BITS: usize = 14;

const MAX_SAMPLE_RATE: u32 = 200_000;

/// (Port, Pin) mapping for AIN0-7
const ADC_INPUTS: &'static [(AnalogPinSelect, u8, u8)] = &[
    (AnalogPinSelect::AnalogInput0, 0, 2),
    (AnalogPinSelect::AnalogInput1, 0, 3),
    (AnalogPinSelect::AnalogInput2, 0, 4),
    (AnalogPinSelect::AnalogInput3, 0, 5),
    (AnalogPinSelect::AnalogInput4, 0, 28),
    (AnalogPinSelect::AnalogInput5, 0, 29),
    (AnalogPinSelect::AnalogInput6, 0, 30),
    (AnalogPinSelect::AnalogInput7, 0, 31)
];

const GAINS: &'static [(GAIN_FIELD, f32)] = &[
    (GAIN_FIELD::Gain1_6, 1.0 / 6.0),
    (GAIN_FIELD::Gain1_5, 1.0 / 5.0),
    (GAIN_FIELD::Gain1_4, 1.0 / 4.0),
    (GAIN_FIELD::Gain1_3, 1.0 / 3.0),
    (GAIN_FIELD::Gain1_2, 1.0 / 2.0),
    (GAIN_FIELD::Gain1, 1.0),
    (GAIN_FIELD::Gain2, 2.0),
    (GAIN_FIELD::Gain4, 4.0),
];


pub struct ADC {
    periph: SAADC
}

#[derive(Clone)]
pub struct ADCChannelConfig {
    pin_select: AnalogPinSelect,
    negative_pin_select: AnalogPinSelect,

    config_value: CONFIG_VALUE,

    /// Note that if a limit is disabled, the value is set to the min/max u16 value
    /// which will never be reached by the ADC so is effectively disabled.
    limit: LIMIT_VALUE,

    /// If true, sampling will stop immediately once one of the LIMIT events is hit.
    /// (this means that it is possible that only a partial window will be read).
    stop_on_limit: bool,
    
    units_per_volt: f32,
}

impl ADCChannelConfig {
    pub fn format(&self) -> ADCFormat {
        let mut proto = ADCFormat::default();
        proto.set_bits_per_sample(RESOLUTION_NUM_BITS as u32);
        proto.set_units_per_volt(self.units_per_volt);
        proto
    }
}

pub struct ADCSampleStatus {
    pub num_samples: usize,
    pub limit_low_exceeded: bool,
    pub limit_high_exceeded: bool
}

impl Drop for ADC {
    fn drop(&mut self) {
        self.periph.enable.write_disabled();
    }
}

impl ADC {
    pub fn new(mut periph: SAADC) -> Self {
        // TODO: Flatten this register field since it is the only one in the register. 
        periph.resolution.write_with(|v| v.set_val_with(|v| v.set_14bit()));

        periph.enable.write_enabled();

        Self { periph }
    }

    pub async fn calibrate_offset(&mut self) {
        // TODO: Should I configure to sampling time to make the calibration more accurate?

        self.periph.events_calibratedone.write_notgenerated();
        flush_events_clear();

        self.periph.tasks_calibrateoffset.write_trigger();

        {
            self.periph.inten.write_with(|v| {
                v.set_calibratedone(InterruptState::Enabled)
            });

            while self.periph.events_calibratedone.read().is_notgenerated() {
                wait_for_irq(Interrupt::SAADC).await;
            }

            self.periph.events_calibratedone.write_notgenerated();
            flush_events_clear();
        }

        // Disable all interrupts.
        self.periph.inten.write_with(|v| v);
    }

    pub fn create_channel_config<Pin: PeripheralPin>(
        &mut self,
        pin: Pin,
        negative_pin: Option<Pin>,
        config: &ConfigureADCRequest
    ) -> Option<ADCChannelConfig> {
        /*
        // Claim an unused channel.
        let index = {
            let mut index = None;
            for i in 0..self.periph.ch.len() {
                if self.periph.ch[i].pselp.read().is_nc() {
                    index = Some(i);
                    break;
                }
            }

            match index {
                Some(v) => v,
                None => return None
            }
        };
        */

        // TODO: Should I prevent multiple channels using the same input?

        let pin_select = match Self::pin_selector(&pin) {
            Some(v) => v,
            None => return None
        };

        let negative_pin_select = match negative_pin.as_ref() {
            Some(p) => match Self::pin_selector(p) {
                Some(v) => v,
                None => return None
            },
            None => AnalogPinSelect::NC 
        };

        // Reference selection
        let mut ref_select = REFSEL_FIELD::Internal;
        let mut ref_voltage = 0.6;
        if config.vdd_reference() {
            ref_select = REFSEL_FIELD::VDD1_4;
            ref_voltage = VDD_VOLTAGE / 4.0;
        }

        // Pick the largest gain that lets us see the whole user range.
        // TODO
        let (mut gain_value, mut gain_ratio) = GAINS[0];

        // User forgot to specify it.
        if config.max_voltage() == 0.0 {
            return None;
        }

        for (gain_value_i, gain_ratio_i) in GAINS.iter().cloned() {
            let max_input_voltage = ref_voltage / gain_ratio_i;
            if max_input_voltage >= config.max_voltage() - 0.01 {
                gain_value = gain_value_i;
                gain_ratio = gain_ratio_i;
            }
        }

        // Based on this formula from the datasheet:
        // RESULT = [V(P) – V(N)] * (GAIN/REFERENCE) * 2^(RESOLUTION - m)
        let units_per_volt = {
            let mut num_units: u32 = 1 << RESOLUTION_NUM_BITS;
            if negative_pin.is_some() {
                num_units >>= 1;
            }
            
            (gain_ratio / ref_voltage) * (num_units as f32)   
        };



        let mut limit_low = -32768i16;
        let mut limit_high = 32767i16;

        if config.has_trigger_above() {
            let v = config.trigger_above() * units_per_volt;
            // TODO: Check for overflow.
            limit_high = v as i16;
        }

        if config.has_trigger_below() {
            let v = config.trigger_below() * units_per_volt;
            // TODO: Check for overflow.
            limit_low = v as i16;
        }

        // Required per the datasheet.
        if limit_high < limit_low {
            return None;
        }

        let mut limit = LIMIT_VALUE::new();
        limit.set_low((limit_low as u16) as u32);
        limit.set_high((limit_high as u16) as u32);

        let resp = match config.pin_ladder() {
            ConfigureADCRequest_ResistorLadder::BYPASS => RESP_FIELD::Bypass,
            ConfigureADCRequest_ResistorLadder::PULL_UP => RESP_FIELD::Pullup,
            ConfigureADCRequest_ResistorLadder::PULL_DOWN => RESP_FIELD::Pulldown,
            ConfigureADCRequest_ResistorLadder::CENTERED => RESP_FIELD::VDD1_2,
        };

        let resn = match config.neg_pin_ladder() {
            ConfigureADCRequest_ResistorLadder::BYPASS => RESN_FIELD::Bypass,
            ConfigureADCRequest_ResistorLadder::PULL_UP => RESN_FIELD::Pullup,
            ConfigureADCRequest_ResistorLadder::PULL_DOWN => RESN_FIELD::Pulldown,
            ConfigureADCRequest_ResistorLadder::CENTERED => RESN_FIELD::VDD1_2,
        };

        let mut config_value = CONFIG_VALUE::new();
        config_value
        .set_gain(gain_value)
        .set_refsel(ref_select)
        /*
        From the datasheet:

        'f_sample < 1 / (t_acq + t_conv)'
            where t_conv < 2us

        So for
            t_acq = 10us, limit is 83k
            t_acq = 5us, limit is 142k
            t_acq = 3us, limit is 200k
        */
        .set_tacq_with(|v| {
            if config.sample_rate() >= 140_000 {
                v.set_3us()
            } else if config.sample_rate() >= 80_000 {
                v.set_5us()
            } else {
                v.set_10us()
            }        
        })
        .set_mode_with(|v| {
            if negative_pin_select != AnalogPinSelect::NC {
                v.set_diff();
            }

            v
        })
        .set_resp(resp)
        .set_resn(resn);

        Some(ADCChannelConfig {
            pin_select,
            negative_pin_select,
            config_value,
            units_per_volt,
            limit,
            stop_on_limit: config.stop_on_trigger(),
        })
    }

    fn pin_selector<Pin: PeripheralPin>(pin: &Pin) -> Option<AnalogPinSelect> {
        let target_port = pin.port() as u8;
        let target_pin_num = pin.pin() as u8;

        ADC_INPUTS
        .iter()
        .find(|(_, port, pin_num)| {
            (*port, *pin_num) == (target_port, target_pin_num)
        })
        .map(|(s, _, _)| s.clone())
    }

    /// Takes a single sample from a single channel.
    ///
    /// TODO: Support cancellation of this future.
    pub async fn single_sample(&mut self, config: &ADCChannelConfig) -> i16 {
        let mut buf = [0i16; 1];
        self.sample_setup(config, OVERSAMPLE_FIELD::Bypass, &mut buf[..]).await;
        self.periph.tasks_sample.write_trigger();
        self.sample_finish(config).await;

        buf[0]
    }

    async fn sample_setup(
        &mut self,
        config: &ADCChannelConfig,
        oversampling: OVERSAMPLE_FIELD,
        buf: &mut [i16]
    ) {
        self.periph.ch[0].pselp.write(config.pin_select);
        self.periph.ch[0].pseln.write(config.negative_pin_select);
        self.periph.ch[0].limit.write(config.limit);
        self.periph.ch[0].config.write(config.config_value);
        self.periph.oversample.write(oversampling);

        // Setup interrupts and clear initial state of events we will use
        self.periph.events_started.write_notgenerated();
        self.periph.events_end.write_notgenerated();
        self.periph.events_stopped.write_notgenerated();

        // Other events we will use.
        self.periph.events_ch[0].limith.write_notgenerated();
        self.periph.events_ch[0].limitl.write_notgenerated();
        // flush_events_clear();

        // Setup buffer.
        self.periph.result.ptr
            .write(unsafe { core::mem::transmute(buf.as_ptr()) });
        self.periph.result.maxcnt
            .write(buf.len() as u32);

        // 'START' to give the buffer to EasyDMA      
        {
            self.periph.tasks_start.write_trigger();
            self.periph.inten.write_with(|v| {
                v.set_started(InterruptState::Enabled)
            });

            while self.periph.events_started.read().is_notgenerated() {
                wait_for_irq(Interrupt::SAADC).await;
            }

            self.periph.events_started.write_notgenerated();
            flush_events_clear();
        }
    }

    async fn sample_finish(&mut self, config: &ADCChannelConfig) -> ADCSampleStatus {

        // Wait for buffer to be filled
        {
            self.periph.inten.write_with(|v| {
                v.set_end(InterruptState::Enabled);

                if config.stop_on_limit {
                    v.set_ch0limith(InterruptState::Enabled)
                      .set_ch0limitl(InterruptState::Enabled);
                }

                v
            });

            loop {
                // Buffer is full.
                if self.periph.events_end.read().is_generated() {
                    break;
                }

                if config.stop_on_limit {
                    // TODO: These may retrigger interrupts.
                    if self.periph.events_ch[0].limith.read().is_generated() {
                        break;
                    }
                    if self.periph.events_ch[0].limitl.read().is_generated() {
                        break;
                    }
                }

                wait_for_irq(Interrupt::SAADC).await;
            }

            self.periph.events_end.write_notgenerated();
            flush_events_clear();

            // TODO: Explicitly clear SAADC interrupts here.
        }

        // From the datasheet: "EasyDMA is finished accessing RAM when events END or STOPPED are generated"
        // TODO: Assert that RESULT.AMOUINT == buf.len()

        // 'STOP' task
        {
            self.periph.inten.write_with(|v| {
                v.set_stopped(InterruptState::Enabled)
            });

            self.periph.tasks_stop.write_trigger();
            while self.periph.events_stopped.read().is_notgenerated() {
                wait_for_irq(Interrupt::SAADC).await;
            }

            self.periph.events_stopped.write_notgenerated();
            flush_events_clear();
        }

        // Disable all interrupts.
        self.periph.inten.write_with(|v| v);

        // Disable ADC
        // self.periph.enable.write_disabled();

        // TODO: Disconnect pins.

        ADCSampleStatus {
            num_samples: self.periph.result.amount.read() as usize,
            limit_high_exceeded: self.periph.events_ch[0].limith.read().is_generated(),
            limit_low_exceeded: self.periph.events_ch[0].limitl.read().is_generated()
        }
    }
}



/*
- Get a 16Mhz 32-bit timer
- Set CC[0] 
- Short EVENTS_COMPARE[0] -> TASKS_CLEAR on the timer
    - will be initially cleared and stopped

- Make a PPI channel
    - Event: EVENTS_COMPARE[0] (on the timer)
    - Task: TASKS_SAMPLE (on the ADC)

- Another PPI
    - Event: EVENTS_END (on the ADC)
    - Task: TASKS_STOP and TASKS_CLEAR (on the timer)


So to start sampling I will:
    - Start timer
    - Trigger the first sample

Then, wait for END

<Rest of cleanup is the same as single sample mode>
*/
/// NOTE: The timer should be in a stopped state when not sampling.
pub struct WindowADC {
    adc: ADC,
    timer: TIMER1,
    sample_ppi: PPIChannel,
    rtc: RTC
}

#[derive(Clone)]
pub struct WindowADCChannelConfig {
    inner: ADCChannelConfig,

    sample_rate: u32,

    oversampling: OVERSAMPLE_FIELD,
}

impl WindowADCChannelConfig {
    pub fn format(&self) -> ADCFormat {
        self.inner.format()
    }
}

// impl_deref!(WindowADC::adc as ADC);

impl WindowADC {
    pub fn create(mut adc: ADC, mut timer: TIMER1, ppi: &mut PPIChannels, rtc: RTC) -> Option<Self> {
        timer.mode.write_timer();
        timer.prescaler.write(0); // 16 MHz
        timer.bitmode.write_32bit();
        timer.tasks_stop.write_trigger();
        timer.tasks_clear.write_trigger();
        timer.shorts.write_with(|v| v.set_compare0_clear_with(|v| v.set_enabled()));

        let mut sample_ppi = match ppi.new_channel(
            &timer.events_compare[0],
            &mut adc.periph.tasks_sample
        ) {
            Some(v) => v,
            None => return None
        };
        sample_ppi.enable();

        Some(Self {
            adc,
            timer,
            sample_ppi,
            rtc
        })
    }

    pub fn create_channel_config<Pin: PeripheralPin>(
        &mut self,
        pin: Pin,
        negative_pin: Option<Pin>,
        config: &ConfigureADCRequest
    ) -> Option<WindowADCChannelConfig> {

        let sample_rate = config.sample_rate();

        let oversampling = match config.oversampling() {
            0 | 1 => OVERSAMPLE_FIELD::Bypass,
            2 => OVERSAMPLE_FIELD::Over2x,
            4 => OVERSAMPLE_FIELD::Over4x,
            8 => OVERSAMPLE_FIELD::Over8x,
            16 => OVERSAMPLE_FIELD::Over16x,
            32 => OVERSAMPLE_FIELD::Over32x,
            64 => OVERSAMPLE_FIELD::Over64x,
            128 => OVERSAMPLE_FIELD::Over128x,
            256 => OVERSAMPLE_FIELD::Over256x,
            _ => return None
        };

        let inner = match self.adc.create_channel_config(pin, negative_pin, config) {
            Some(v) => v,
            None => return None
        };


        Some(WindowADCChannelConfig {
            inner,
            oversampling,
            sample_rate
        })

    }

    pub async fn calibrate_offset(&mut self) {
        self.adc.calibrate_offset().await
    }

    pub async fn single_sample(&mut self, config: &WindowADCChannelConfig) -> i16 {
        if config.oversampling == OVERSAMPLE_FIELD::Bypass {
            return self.adc.single_sample(&config.inner).await;
        }

        let mut buf = [0i16; 1];
        self.window_sample(config, &mut buf).await;
        buf[0]
    }

    // TODO: Make this cancellable.
    pub async fn window_sample(
        &mut self,
        config: &WindowADCChannelConfig,
        out: &mut [i16]
    ) -> Option<ADCSampleStatus> {
        if out.len() > 1 || config.oversampling != OVERSAMPLE_FIELD::Bypass {
            if config.sample_rate < 10 || config.sample_rate > MAX_SAMPLE_RATE {
                return None;
            }
        }

        let mut sample_rate = config.sample_rate;
        if sample_rate == 0 {
            sample_rate = 1;
        }

        self.timer.cc[0].write(16_000_000 / sample_rate);

        self.adc.sample_setup(&config.inner, config.oversampling, out).await;

        // Start first sample. All other samples will be timer triggered.
        self.adc.periph.tasks_sample.write_trigger();
        self.timer.tasks_start.write_trigger();

        let res = self.adc.sample_finish(&config.inner).await;

        // Note that the timer still runs for a short time after the last sample
        // is stored so the safety of having these here (instead of using a PPI to immediately
        // stop the timer on the ADC END event) is that the ADC TASKS_SAMPLE doesn't do anything
        // if the buffer is full or the ADC is stopped.
        self.timer.tasks_stop.write_trigger();
        self.timer.tasks_clear.write_trigger();

        Some(res)
    }
}


