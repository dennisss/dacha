use crate::thermal_model::ThermalFEM;
use crate::bed::training_data::*;

const AMBIENT_TEMP: f32 = 24.0;

#[derive(Clone)]
pub struct BedThermalFEM {
    pub fem: ThermalFEM,
    pub bottom_el: usize,
    pub middle_el: usize,
    pub top_el: usize,
    pub air_el: usize,
    pub heater_coeff: f32,
    pub fan_coeff: f32,
    pub fan_relation_idx: usize,
}

impl BedThermalFEM {
    pub fn create(weights: &[f32]) -> Self {
        let mut fem = ThermalFEM::default();

        let bottom_el = fem.add_element(AMBIENT_TEMP);
        let middle_el = fem.add_element(AMBIENT_TEMP);
        let top_el = fem.add_element(AMBIENT_TEMP);
        let air_el = fem.add_element(AMBIENT_TEMP);
        // let sheet_el = fem.add_element(AMBIENT_TEMP);

        // Heater
        fem.model.sources.push((bottom_el, 0.0));

        fem.model.relations.push((bottom_el, middle_el, weights[1]));
        fem.model.relations.push((middle_el, bottom_el, weights[1]));

        fem.model.relations.push((top_el, middle_el, weights[1]));
        fem.model.relations.push((middle_el, top_el, weights[1]));

        /*
        Surface Areas:
        - Bed is 120mm x 120mm x ~10mm
            - 0.0144 m^2 on top/bottom
            - 0.0016 m^2 for each third on the sides
        */

        let large_area = 0.0144;
        let small_area = 0.0016;

        let fan_relation_idx = fem.model.relations.len();
        fem.model.relations.push((air_el, bottom_el, 0.0)); // Fan initially off.
        let fan_coeff = (large_area + small_area) * weights[3];

        fem.model.relations.push((air_el, bottom_el, (large_area + small_area) * weights[2]));
        fem.model.relations.push((air_el, top_el, (large_area + small_area) * weights[2]));
        fem.model.relations.push((air_el, middle_el, (large_area + small_area) * weights[2]));

        Self {
            fem,
            bottom_el,
            middle_el,
            top_el,
            air_el,
            heater_coeff: weights[0],
            fan_coeff,
            fan_relation_idx
        }
    }

    pub fn set_heater(&mut self, duty_cycle: f32) {
        self.fem.model.sources[0].1 = duty_cycle * self.heater_coeff;
    }

    pub fn set_fan(&mut self, duty_cycle: f32) {
        self.fem.model.relations[self.fan_relation_idx].2 = duty_cycle * self.fan_coeff;
    }

    /// Simulates forward in time over all timesteps in 'data' and calculates the difference
    /// between the simulated results and measured results (from the data).
    /// 
    /// Assumptions:
    /// - Initial state of the FEM is the same as data.rows[0]
    pub fn calculate_error(
        mut self,
        data: &BedTrainingData,
        mut result: Option<&mut BedTrainingData>
    ) -> f32 {
        let mut error = 0.0;
        let mut time = data.rows[0].time;

        for row in &data.rows {
            let dt = row.time - time;
            self.fem.step(dt);

            // TODO: Use trapezoidal error.
            let mut e = 0.0;
            if let Some(t) = row.sheet {
                e += squared(self.fem.elements[self.top_el] - t);
            }
            if let Some(t) = row.bed {
                e += squared(self.fem.elements[self.middle_el] - t);
            }

            error += dt * e;

            self.set_heater(row.heater);
            self.set_fan(row.fan);

            if let Some(ref mut result) = result {
                result.rows.push(BedTrainingDataRow {
                    heater: row.heater,
                    time: row.time,
                    fan: row.fan,
                    sheet: Some(self.fem.elements[self.top_el]),
                    bed: Some(self.fem.elements[self.middle_el]),
                });
            }

            time = row.time;
        }

        error
    }
}

fn squared(v: f32) -> f32 {
    v * v
}
