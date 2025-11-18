use crate::bed::training_data::*;
use crate::bed::thermal_model::*;


pub fn calculate_error(weights: &[f32], data: &BedTrainingData, mut result: Option<&mut BedTrainingData>) -> f32 {
    calculate_error_fem(BedThermalFEM::create(weights), data, result)
}

pub fn calculate_error_fem(
    mut bed_fem: BedThermalFEM,
    data: &BedTrainingData,
    mut result: Option<&mut BedTrainingData>
) -> f32 {
    bed_fem.calculate_error(data, result)
}
