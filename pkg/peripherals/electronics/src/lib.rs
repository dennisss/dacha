mod voltage_divider;
mod pt1000;
mod ntc;
mod thermistor;

pub use voltage_divider::*;
pub use pt1000::*;
pub use ntc::*;
pub use thermistor::*;


pub fn thermistor_by_name(name: &str) -> Option<Box<dyn Thermistor>> {
    Some(match name {
        "PT1000" => {
            Box::new(PT1000::default())
        }
        "104NT-4-R025H42G" => {
            Box::new(Thermistor104NT::default())
        }
        _ => return None
    })
}