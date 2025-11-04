
/// Computes the output voltage of a voltage divider.
///
/// Should be of the form:
/// v_in -> r_upper -> v_out -> r_lower -> 0V
pub fn divide_voltage(v_in: f32, r_upper: f32, r_lower: f32) -> f32 {
    (v_in * r_lower) / (r_upper + r_lower)
}

pub fn undivide_voltage(v_out: f32, r_upper: f32, r_lower: f32) -> f32 {
    (v_out * (r_upper + r_lower)) / r_lower
}

/// Given the input/output voltage and upper resistor value in a voltage divider,
/// calculates the lower resistor value.
pub fn undivide_voltage_lower(v_in: f32, v_out: f32, r_upper: f32) -> f32 {
    (v_out * r_upper) / (v_in - v_out)
}
