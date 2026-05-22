use base_algorithms::binary_search::*;

/// Mapping from forward voltage to current for a single SFH 4715AS A01
/// (data from the datasheet)
const SINGLE_LED_VOLTAGE_VS_CURRENT: &'static [(f32, f32)] = &[
    (2.5, 0.01),
    (2.6, 0.03),
    (2.7, 0.1),
    (2.8, 0.4),
    (2.9, 0.75),
    (3.0, 1.5),
    (3.1, 2.0),
    (3.2, 2.5),
    (3.3, 3.0),
    (3.4, 4.0),
];

const NUM_LEDS: usize = 12;

/// Simulation model for the efuse + led driver + bulk capacitor
/// on the LED strobe boards.
pub struct StrobeSimulationModel {
    inductor_value: f32,
    capacitor_value: f32,
    driver_efficiency: f32,
    driver_resistance: f32,
    input_voltage: f32,
    input_current_limit: f32,
    current_sense_resistor: f32,

    /// Current at 100% requested power.
    current_scale: f32,
}

#[derive(Clone)]
pub struct StrobeSettings {
    pub frequency: f32,
    pub pulse_width: f32,
    pub power_percent: f32, 
}

#[derive(Debug)]
pub struct StrobeSimulationResult {
    pub success: bool,
    
    /// Amount of time that it took to reach peak current (in seconds).
    pub charge_time: f32,

    /// Whether or not we hit the peak current requested.
    /// If success = true, and this is false, then the pulse is not long
    /// enough for 
    pub target_reached: bool,

    pub peak_current: f32,

    pub period_end_voltage: f32,

    pub average_strobe_power: f32,
}

struct StrobeSimulationState {
    capacitor_voltage: f32,
    inductor_current: f32,
}

impl StrobeSimulationModel {

    pub fn settings_r1() -> Self {
        Self {
            inductor_value: 22.0e-6, // 22uH
            capacitor_value: 220.0e-6, // 220uF
            driver_efficiency: 0.9,
            driver_resistance: 0.150,
            input_voltage: 52.0,
            input_current_limit: 0.2,
            current_sense_resistor: 0.05,
            current_scale: 4.0,
        }
    }

    /// Returns a clamped version of settings.power_percent that is in the feasible
    /// range 
    pub fn clamp_power_percent(
        &self,
        settings: &StrobeSettings,
        safety_margin: f32
    ) -> f32 {
        let v = settings.power_percent.min(1.0).max(0.0);

        // Fast path is when the given value is already good.
        let mut settings_test = settings.clone();
        settings_test.power_percent = v + safety_margin;
        if self.run(&settings_test).success {
            return v;
        }

        println!("AAA");

        let limit = self.find_power_limit(settings.frequency, settings.pulse_width, safety_margin);
        v.min(limit)
    }

    pub fn find_power_limit(
        &self,
        frequency: f32,
        pulse_width: f32,
        safety_margin: f32
    ) -> f32 {
        let mut search = BinarySearch::new(0, 20);
        while !search.done() {
            let res = self.run(&StrobeSettings {
                frequency,
                pulse_width,
                power_percent: ((search.current() as f32) * 0.05) + safety_margin
            });

            if res.success {
                search.greater_eq_current();
            } else {
                search.less_than_current();
            }
        }

        (search.best().unwrap_or(0) as f32) * 0.05
    }

    /// Checks if a given set of strobe settings are achievable without faults. This
    /// requires two criteria to be true:
    ///
    /// 1. The inductor current is never decaying
    ///    (also means that at the end of the pulse, the capacitor voltage is still
    ///     higher than the LED voltage + driver losses and the inductor is still at
    ///     peak current) 
    ///
    /// 2. Before the next pulse is meant to start, the capacitor has recharged to the
    ///    full input voltage (else it is decaying between pulses so will eventually
    ///    cause the strobes to fail.)
    ///
    /// Note: Success does not require that we actually reach the target current since
    /// if the pulse width is too short, we may not have enough time to saturate the
    /// inductor.
    ///
    /// TODO: Ahead of time validate that pulse_width < '1/frequency'
    /// TODO: Also simulate the capacitors across the LED strip. 
    pub fn run(&self, settings: &StrobeSettings) -> StrobeSimulationResult {
        if settings.frequency == 0.0 {
            return StrobeSimulationResult {
                success: true,
                charge_time: 0.0,
                target_reached: false,
                peak_current: 0.0,
                period_end_voltage: self.input_voltage,
                average_strobe_power: 0.0
            };
        }
        
        // Assume the capacitor starts fully charged / LEDs fully off.
        let mut state = StrobeSimulationState {
            capacitor_voltage: self.input_voltage,
            inductor_current: 0.0
        };

        let target_current = settings.power_percent * self.current_scale;

        let period = 1.0 / settings.frequency;

        let dt = 0.5e-6; // 0.5us

        let mut t = 0.0;

        // Simulate one full cycle.
        let end_time = period;

        // Metrics
        let mut peak_current = 0.0;
        let mut charge_time = 0.0;
        let mut success = true;
        let mut average_strobe_power = 0.0;

        let inv_inductor_value = 1.0 / self.inductor_value;
        let inv_capacitor_value = 1.0 / self.capacitor_value;
        let inv_driver_efficiency = 1.0 / self.driver_efficiency;

        for i in 0..((end_time / dt).floor() as usize) {
            let t = (i as f32) * dt;

            // let on = (t % period) < settings.pulse_width;
            let on = t < settings.pulse_width;

            if on {
            }

            // Calculate current flowing out of the capacitor
            // (and update the inductor current)
            let i_cap_out = {
                if on {
                    // Voltage across just the LEDs.
                    let v_led = self.led_forward_voltage(state.inductor_current);

                    average_strobe_power += dt * state.inductor_current * v_led;

                    // Voltage across the mosfet in the LED driver.
                    let v_mosfet = self.driver_resistance * state.inductor_current;

                    let v_sense = self.current_sense_resistor * state.inductor_current;

                    if state.capacitor_voltage > v_led {
                        // Capacitor voltage is high enough to drive the LEDs.

                        if state.inductor_current < target_current {
                            // Ramping to max current
                            let di_dt = (state.capacitor_voltage - v_led - v_mosfet - v_sense) * inv_inductor_value;
                            state.inductor_current += di_dt * dt;
                            state.inductor_current = state.inductor_current.min(target_current);

                            state.inductor_current
                        } else {
                            // Holding max currnet
                            state.inductor_current = target_current;

                            // Continuous power draw
                            // We assume that MOSFET switching / sense loses are summarized in the
                            // datasheet efficiency number.
                            let mut p_led = v_led * state.inductor_current;
                            p_led *= inv_driver_efficiency;

                            // Convert power to current
                            p_led / state.capacitor_voltage
                        }
                    } else {
                        // Capacitor is too low to drive the LED strip so the
                        // inductor will just decay.

                        let di_dt = (state.capacitor_voltage - v_led - v_mosfet - v_sense) * inv_inductor_value;
                        state.inductor_current += di_dt * dt;
                        state.inductor_current = state.inductor_current.max(0.0);

                        success = false;

                        0.0
                    }
                } else {
                    // Assume inductor immediately drains when the driver is turned off.
                    state.inductor_current = 0.0;

                    0.0
                }
            };


            

            let input_current = {
                if state.capacitor_voltage < self.input_voltage {
                    // Capacitor recharging (pull max current)
                    self.input_current_limit
                } else {
                    self.input_current_limit.min(i_cap_out)
                }
            };

            // TODO: When the strip is off, integrating this is trivial and can be done in one large timestep.
            let dvc_dt = (input_current - i_cap_out) * inv_capacitor_value;
            state.capacitor_voltage += dvc_dt * dt;

            // Clamp to input voltage.
            state.capacitor_voltage = state.capacitor_voltage
                .min(self.input_voltage);

            if state.inductor_current > peak_current {
                peak_current = state.inductor_current;
                charge_time = t + dt;
            }
        }


        success &= state.capacitor_voltage == self.input_voltage;

        StrobeSimulationResult {
            success,
            charge_time,
            target_reached: peak_current == target_current,
            peak_current,
            period_end_voltage: state.capacitor_voltage,
            average_strobe_power: average_strobe_power / settings.pulse_width
        }
    } 

    fn led_forward_voltage(&self, current: f32) -> f32 {
        (NUM_LEDS as f32) * self.single_led_forward_voltage(current)
    }

    fn single_led_forward_voltage(&self, current: f32) -> f32 {
        if current <= 0.0 {
            return 0.0;
        }

        let idx = common::algorithms::upper_bound_by(SINGLE_LED_VOLTAGE_VS_CURRENT, current, |(_, current), c| {
            *current <= c
        });

        let idx = match idx {
            Some(v) => v,
            None => return SINGLE_LED_VOLTAGE_VS_CURRENT[0].0
        };
        
        if idx == SINGLE_LED_VOLTAGE_VS_CURRENT.len() - 1 {
            return SINGLE_LED_VOLTAGE_VS_CURRENT[idx].0;
        }

        let a = &SINGLE_LED_VOLTAGE_VS_CURRENT[idx];
        let b = &SINGLE_LED_VOLTAGE_VS_CURRENT[idx + 1];

        interp(
            a.0, b.0, (b.1 - current) / (b.1 - a.1)
        )
    }

}


// TODO: Dedup this.
fn interp(a: f32, b: f32, a_alpha: f32) -> f32 {
    a * a_alpha + b * (1.0 - a_alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Instant;

    #[test]
    fn works() {

        let model = StrobeSimulationModel::settings_r1();

        assert_eq!(model.single_led_forward_voltage(0.0001), 2.5);
        assert_eq!(model.single_led_forward_voltage(5.0), 3.4);
        assert_eq!(model.single_led_forward_voltage(2.0), 3.1);
        assert!((model.single_led_forward_voltage(3.5) - 3.35).abs() < 0.001);

        println!("6ms. 1A:  {:?}", model.run(&StrobeSettings {
            frequency: 24.0,
            pulse_width: 6000.0e-6,
            power_percent: 0.25,
        }));

        let a = Instant::now();
        let limit1 = model.find_power_limit(240.0, 250.0e-6, 0.05);
        let limit2 = model.find_power_limit(120.0, 250.0e-6, 0.05);
        let b = Instant::now();

        println!("Limit 240hz : {}", limit1);
        println!("Limit 120hz : {}", limit2);

        let limit3 = model.find_power_limit(24.0, 6000.0e-6, 0.05);
        println!("Limit 24hz: {}", limit3);

        println!("{:?}", b - a);

    }

}
