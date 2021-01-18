use crate::avr::interrupts::*;
use crate::avr::registers::*;
use core::ptr::{read_volatile, write_volatile};

pub struct ADC {}

impl ADC {
    #[inline(never)]
    fn start(input: ADCInput) {
        // REFS1:0: AVcc with external capacitor on AREF pin
        // ADLAR = 0
        let mut admux = 0b01 << 6;
        admux |= input as u8;
        let adcsrb = 0;
        // ADC Enable | ADC Start Conversion | ADC Interrupt Enable | 128 clock divison.
        let adcsra = 1 << 7 | 1 << 6 | 1 << 3 | 0b111;
        unsafe {
            write_volatile(ADMUX, admux);
            write_volatile(ADCSRB, adcsrb);
            write_volatile(ADCSRA, adcsra);
        }

        // After this, we must wait for the interrupt to trigger in order to
        // read the value.
    }
    fn read_result() -> u16 {
        let (low, high) = unsafe {
            let low = read_volatile(ADCL);
            let high = read_volatile(ADCH);
            (low, high)
        };
        (low as u16) | (((high & 0b11) as u16) << 8)
    }
    pub async fn read(input: ADCInput) -> u16 {
        Self::start(input);
        InterruptEvent::ADCComplete.await;
        Self::read_result()
    }
}

/// NOTE: The associated u8 values are the values needed for the ADMUX::MUX
/// register bits.
pub enum ADCInput {
    ADC0 = 0b000000,
    ADC1 = 0b000001,
    ADC4 = 0b000100,
    ADC5 = 0b000101,
    ADC6 = 0b000110,
    ADC7 = 0b000111,
    V1_1 = 0b011110,
    GND = 0b011111,
    TEMP = 0b100111,
}
