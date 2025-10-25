

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

const PT1000_R0: f32 = 1000.0;
const PT1000_A: f32 = 3.9083e-3;
const PT1000_B: f32 = -5.775e-7;
const PT1000_C: f32 = -4.183e-12;

/// Model of a PT1000 thermistor.
/// Valid from 0C to 850C
///
/// Rt = R0 * (1 + A*t + B*t^2)
pub struct PT1000 {}

impl PT1000 {
    /// Converts a temperature in celsius to a resistance in ohms.
    pub fn temperature_to_resistance(t: f32) -> f32 {
        PT1000_R0 * (1.0 + PT1000_A * t + PT1000_B * t * t)
    }

    pub fn resistance_to_temperature(r: f32) -> f32 {
        let (t1, t2) = math::find_quadratic_roots(PT1000_B * PT1000_R0, PT1000_A * PT1000_R0, PT1000_R0 - r);
        t1.min(t2)
    }
}


/// Computes the output voltage of a voltage divider.
///
/// Should be of the form:
/// v_in -> r_upper -> v_out -> r_lower -> 0V
pub fn divide_voltage(v_in: f32, r_upper: f32, r_lower: f32) -> f32 {
    (v_in * r_lower) / (r_upper + r_lower)
}

/// Given the input/output voltage and upper resistor value in a voltage divider,
/// calculates the lower resistor value.
pub fn undivide_voltage_lower(v_in: f32, v_out: f32, r_upper: f32) -> f32 {
    (v_out * r_upper) / (v_in - v_out)
}


