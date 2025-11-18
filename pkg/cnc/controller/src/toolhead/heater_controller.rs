use std::time::{Instant, Duration};

use common::errors::*;

use crate::toolhead::thermal_model::*;
use crate::toolhead::training_data::*;
use crate::optimizer::*;


/// Number of seconds into the future that we should plan.
const TIME_HORIZON: f32 = 15.0;

/// Number of seconds between changes to the control inputs.
const CONTROL_INTERVAL: f32 = 1.0;

pub struct ToolheadHeaterController {
    /// Last time at which we did planning.
    time: Instant,

    /// State of the system at 'time'
    state: ToolheadThermalModel,

    /// Control input (heater duty cycles) we want issued over time.
    ///
    /// - The length is proportion to the time horizon.
    /// - inputs.rows[0] has the control input to be issued immediately at 'time'.
    /// - The target temperature is stored in 'nozzle_temp'
    ///
    /// TODO: Also feed planned fan speed and extrusion changes into here. 
    inputs: ToolheadTrainingData,
}

impl ToolheadHeaterController {

    // TODO: Consider also using the realtime current sense data.

    // TODO: Also feed in the control interval from the caller.
    pub fn create(
        thermal_model_weights: &[f32],
        initial_temperature: f32,
    ) -> Self {

        let time = Instant::now();

        let mut state = ToolheadThermalModel::create(thermal_model_weights);

        // Assuming that the nozzle and heater are the same temperature initially.
        state.fem.elements[state.nozzle_el] = initial_temperature;
        state.fem.elements[state.ring_el] = initial_temperature;


        let mut inputs = ToolheadTrainingData::default();

        let mut t = 0.0;
        while t < TIME_HORIZON {
            inputs.rows.push(ToolheadTrainingDataRow {
                time: t,
                heater: 0.0,
                heater_temp: None,
                fan: 0.0,
                nozzle_temp: Some(0.0),
                heater_current: 0.0,
                heater_voltage: 0.0,
                psu_current: 0.0,
                psu_voltage: 0.0,
            });

            t += CONTROL_INTERVAL;
        }


        Self {
            time,
            state,
            inputs,
        }
    }

    pub fn set_target_nozzle_temperature(&mut self, temp: f32) {
        for row in &mut self.inputs.rows {
            row.nozzle_temp = Some(temp);
        }
    }

    /// Gets the next heater duty cycle to set at the current point in time.
    ///
    /// 'current_heater_temperature' should be the most recently measured temperature
    /// of the heater.
    ///
    /// NOTE: 'current_heater_temperature' is the temp of the heater while the target
    /// temperature we are controlling is the separate 'nozzle' element.
    pub async fn next_control_input(&mut self, current_heater_temperature: f32) -> Result<f32> {

        // Advance the state.
        let now = Instant::now();
        // TODO: Complain if this is a large time period (e.g. we missed a sample).
        // TODO: If this is a long period of time, then we need to vary the heater power.
        self.state.fem.step((now - self.time).as_secs_f32());

        // Correct state based on latest measurement
        self.state.fem.elements[self.state.ring_el] = current_heater_temperature;

        // TODO: Should I predict ahead a little bit more since there will be some delay needed to compute the control input and send it to the MCU?

        self.time = now;

        // TODO: Gradient descent the inputs.
        // TODO: Bound the number of steps / time.
        let mut options = OptimizerOptions::default();
        options.monitor_steps_interval = 20;
        options.max_steps = Some(200);
        options.print_progress = false;
        gradient_descent(self, options).await?;

        let v = self.inputs.rows[0].heater;
        self.state.set_heater(v);

        // NOTE: At this point we should technically remove the first row and append a new
        // row at the end, but the control inputs are smooth enough that the next gradient
        // descent round for ripple things appropriately.

        Ok(v)
    }
}

impl OptimizerInput for ToolheadHeaterController {
    fn weight(&self, i: usize) -> f32 {
        self.inputs.rows[i].heater
    }

    fn set_weight(&mut self, i: usize, v: f32) {
        self.inputs.rows[i].heater = v;
    }

    fn weights_len(&self) -> usize {
        // NOTE: The last input doesn't do anything since it is the starting point of a unconcluded heating period.
        self.inputs.rows.len() - 1
    }

    fn clamp_weight(&self, i: usize, v: f32) -> f32 {
        v.max(0.0).min(1.0)
    }

    fn debug_weights(&self) -> String {
        let mut values = self.inputs.rows.iter().map(|v| v.heater).collect::<Vec<f32>>();
        format!("{:.2?}", values)
    }

    fn learning_rate(&self) -> f32 {
        0.00002
        // 0.0001
    }

    fn calculate_error(&mut self, step: Option<usize>) -> f32 {
        let model = self.state.clone();
        model.calculate_error(&self.inputs, None)
    }
}
