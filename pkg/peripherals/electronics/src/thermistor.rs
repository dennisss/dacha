
pub trait Thermistor: Send + Sync {
    /// Converts a temperature in celsius to a resistance in ohms.
    fn temperature_to_resistance(&self, t: f32) -> Option<f32>;

    fn resistance_to_temperature(&self, r: f32) -> Option<f32>;
}
