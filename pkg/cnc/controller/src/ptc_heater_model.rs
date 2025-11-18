use crate::toolhead::training_data::*;
use crate::optimizer::*;

/// Model of a PTC heater. 'Power = f(Temperature)'
#[derive(Clone, Debug)]
pub struct PTCHeaterModel {
    pub offset: f32,
    pub coeff: f32,
    pub coeff2: f32,
}

impl PTCHeaterModel {
    pub fn predict_power(&self, temp: f32) -> f32 {
        // self.offset * ((-self.coeff / 10000.0) * temp).exp()

        self.offset * (
            ((-self.coeff / 10000.0) * temp).exp()
            // + ((-self.coeff2 / 1000000.0) * squared(temp)).exp()

        )

    }
}

pub struct PTCHeaterOptimizerInput {
    data: ToolheadTrainingData,
    model: PTCHeaterModel
}

impl PTCHeaterOptimizerInput {
    pub fn create(mut data: ToolheadTrainingData) -> Self {
        data.rows.retain(|row| {
            row.heater > 0.3 && row.heater_current > 0.0
        });
        
        // TODO: Deduplicate data that is near the same temperature? (or decrease the weighting in training)

        Self {
            data,
            model: PTCHeaterModel {
                offset: 54.0,
                coeff: 10000.0 * 0.00114,
                coeff2: 1000000.0 * 0.000001
            }
        }
    }
    
    pub fn model(&self) -> &PTCHeaterModel {
        &self.model
    }

}

impl OptimizerInput for PTCHeaterOptimizerInput {
    fn weight(&self, i: usize) -> f32 {
        match i {
            0 => { self.model.offset },
            1 => { self.model.coeff },
            2 => { self.model.coeff2 },
            _ => panic!()
        }
    }

    fn set_weight(&mut self, i: usize, v: f32) {
        match i {
            0 => { self.model.offset = v; },
            1 => { self.model.coeff = v; },
            2 => { self.model.coeff2 = v; },
            _ => panic!()
        }
    }

    fn weights_len(&self) -> usize {
        2
    }

    fn clamp_weight(&self, i: usize, v: f32) -> f32 {
        v.max(0.0)
    }

    fn debug_weights(&self) -> String {
        format!("{:.3} {:.3} {:.2}", self.model.offset, self.model.coeff, self.model.coeff2)
    }

    fn learning_rate(&self) -> f32 {
        0.0000001
    }

    fn calculate_error(&mut self, step: Option<usize>) -> f32 {
        let mut error = 0.0;

        for row in &self.data.rows {
            let v = self.model.predict_power(row.heater_temp.unwrap());
            let actual_v = (row.heater_current * row.heater_voltage) / row.heater;
            error += squared(v - actual_v);
        }

        error
    }

}

fn squared(v: f32) -> f32 {
    v * v
}