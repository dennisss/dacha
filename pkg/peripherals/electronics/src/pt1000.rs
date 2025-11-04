/*

PT1000

0C   : 1000 ohm
25C  : 1097.3 ohm
100C : 1385.1 ohm
150C : 1573.3 ohm

With 4.7K resistor:
    0C   : Vcc * 0.1754385965 (At 5V Vcc this is 0.877192982)
    25C  : Vcc * 0.1892777672 (At 5V Vcc this is )
    100C : Vcc * 0.2276215674 (At 5V Vcc this is 1.13810784)
    150C : Vcc * 0.2507930435 (At 5V Vcc this is 1.25396522)

    ATTiny412 ADC is 10-bit so between 0C and 100C is 53 steps of resolution if using a 5V reference.

    With 1.5V reference, you'd get 178 steps of resolution

    


With 5V supply, my error is 0.0048828125 V
                            0.0029296875

So, I may get a value of 1.5 - 1.5048828125

*/

use crate::thermistor::*;


const PT1000_R0: f32 = 1000.0;
const PT1000_A: f32 = 3.9083e-3;
const PT1000_B: f32 = -5.775e-7;
const PT1000_C: f32 = -4.183e-12;

/// Model of a PT1000 thermistor.
/// Valid from 0C to 850C
///
/// Rt = R0 * (1 + A*t + B*t^2)
#[derive(Default)]
pub struct PT1000 {}

impl Thermistor for PT1000 {
    fn temperature_to_resistance(&self, t: f32) -> Option<f32> {
        Some(PT1000_R0 * (1.0 + PT1000_A * t + PT1000_B * t * t))
    }

    fn resistance_to_temperature(&self, r: f32) -> Option<f32> {
        let (t1, t2) = math::find_quadratic_roots(PT1000_B * PT1000_R0, PT1000_A * PT1000_R0, PT1000_R0 - r);
        Some(t1.min(t2))
    }
}

