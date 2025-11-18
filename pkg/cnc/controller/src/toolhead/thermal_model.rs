use crate::thermal_model::ThermalFEM;
use crate::toolhead::training_data::*;
use crate::ptc_heater_model::*;
use crate::optimizer::OptimizerInput;

const AMBIENT_TEMP: f32 = 24.0;

//  

/*
First model:

- 'Heater'

- 'Outer Ring'
    - Thermistor measures here
    - C: Transfer coefficient to the nozzle
- 'Nozzle'
    - Thermocouple measures here
- 'Air'
    - D: Transfer coefficient to the 
    - (also includes the disipation through the heatsink)
- `Fan`
    - Take heat away from the nozzle



Weights:
[0] Heater <> Ring transfer rate
[1] Ring <> Nozzle transfer rate
[2] Nozzle > Air transfer rate
[3] Fan coefficient
    
[3] Ring > Air transfer rate

*/

// TODO: Make most things private.

#[derive(Clone)]
pub struct ToolheadThermalModel {
    pub fem: ThermalFEM,

    pub heater_model: PTCHeaterModel,
    pub heater_coeff: f32,
    pub heater_duty_cycle: f32,
    
    pub fan_coeff: f32,
    pub fan_relation_idx: usize,

    pub ring_el: usize,
    pub nozzle_el: usize,
    pub air_el: usize,
}

impl ToolheadThermalModel {
    pub fn create(weights: &[f32]) -> Self {
        let mut fem = ThermalFEM::default();

        let heater_model = PTCHeaterModel {
            offset: 53.003498, coeff: 11.193065, coeff2: 0.0
        };

        let ring_el = fem.add_element(AMBIENT_TEMP);
        let nozzle_el = fem.add_element(AMBIENT_TEMP);
        let air_el = fem.add_element(AMBIENT_TEMP);        

        // Heater
        fem.model.sources.push((ring_el, 0.0));

        fem.model.relations.push((ring_el, nozzle_el, weights[1]));
        fem.model.relations.push((nozzle_el, ring_el, weights[1]));

        let fan_relation_idx = fem.model.relations.len();
        fem.model.relations.push((air_el, nozzle_el, 0.0)); // Fan initially off.

        let air_coeff = weights[2] / 100.0;
        let fan_coeff = weights[3] / 100.0;

        fem.model.relations.push((air_el, nozzle_el, air_coeff));
        fem.model.relations.push((air_el, ring_el, air_coeff));

        Self {
            fem,
            heater_model,
            heater_coeff: weights[0],
            heater_duty_cycle: 0.0,
            fan_coeff,
            fan_relation_idx,
            ring_el,
            nozzle_el,
            air_el,
        }
    }

    /// TODO: Ensure that we continously call this if we make large time steps in the simulation.
    pub fn set_heater(&mut self, duty_cycle: f32) {
        self.heater_duty_cycle = duty_cycle;
        
        // TODO: Better parameterize this normalization
        self.fem.model.sources[0].1 = duty_cycle * self.heater_coeff *
            self.heater_model.predict_power(self.fem.elements[self.ring_el]) / 60.0;

        // self.fem.model.sources[0].1 = duty_cycle * self.heater_coeff;
    }

    pub fn set_fan(&mut self, duty_cycle: f32) {
        self.fem.model.relations[self.fan_relation_idx].2 = duty_cycle * self.fan_coeff;
    }

    pub fn calculate_error(
        mut self,
        data: &ToolheadTrainingData,
        mut result: Option<&mut ToolheadTrainingData>
    ) -> f32 {
        let mut error = 0.0;
        let mut time = data.rows[0].time;

        for row in &data.rows {
            let dt = row.time - time;
            self.fem.step(dt);

            // TODO: Use trapezoidal error.
            let mut e = 0.0;

            if let Some(t) = row.heater_temp {
                e += squared(self.fem.elements[self.ring_el] - t);
            }
            
            if let Some(t) = row.nozzle_temp {
                e += squared(self.fem.elements[self.nozzle_el] - t);
            }

            error += dt * e;

            self.set_heater(row.heater);
            self.set_fan(row.fan);

            if let Some(ref mut result) = result {
                result.rows.push(ToolheadTrainingDataRow {
                    time: row.time,
                    heater: row.heater,
                    heater_temp: Some(self.fem.elements[self.ring_el]),
                    fan: row.fan,
                    nozzle_temp: Some(self.fem.elements[self.nozzle_el]),
                    heater_current: 0.0,
                    heater_voltage: 0.0,
                    psu_current: 0.0,
                    psu_voltage: 0.0,
                });
            }

            time = row.time;
        }

        error
    }

}


pub struct ToolheadThermalOptimizerInput {
    data: ToolheadTrainingData,
    weights: Vec<f32>
}

impl ToolheadThermalOptimizerInput {
    pub fn create(data: ToolheadTrainingData) -> Self {

        Self {
            data,
            // weights: vec![1.0, 0.0, 0.0, 0.0]

            // weights: vec![0.4992, 0.0486, 0.0000, 0.0000]
            weights: vec![15.93045, 0.2063875, 0.52236676, 0.678516]
        }

    }

    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

}


impl OptimizerInput for ToolheadThermalOptimizerInput {
    fn weight(&self, i: usize) -> f32 {
        self.weights[i]
    }

    fn set_weight(&mut self, i: usize, v: f32) {
        self.weights[i] = v;
    }

    fn weights_len(&self) -> usize {
        self.weights.len()
    }

    fn clamp_weight(&self, i: usize, v: f32) -> f32 {
        v.max(0.0)
    }

    fn debug_weights(&self) -> String {
        format!("{:?}", self.weights)
    }

    fn learning_rate(&self) -> f32 {
        // First ~8K steps
        // 0.000000000001

        0.0000000001
    }

    fn calculate_error(&mut self, step: Option<usize>) -> f32 {
        let model = ToolheadThermalModel::create(&self.weights);
        model.calculate_error(&self.data, None)
    }
}


fn squared(v: f32) -> f32 {
    v * v
}
