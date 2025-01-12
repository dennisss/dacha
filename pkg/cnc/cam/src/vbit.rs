use std::f32::consts::PI;

pub fn calculate_engraving_diameter(angle_degrees: f32, base_diameter: f32, depth: f32) -> f32 {
    let angle_rads = angle_degrees * (PI / 180.0);
    let half_angle_rads = angle_rads / 2.0;

    let tan = half_angle_rads.tan();

    let base_depth = (base_diameter / 2.0) / tan;

    let full_depth = base_depth + depth;

    let full_radius = full_depth * tan;

    full_radius * 2.0
}

pub fn calculate_engraving_depth(angle_degrees: f32, base_diameter: f32, cut_diameter: f32) -> f32 {
    let angle_rads = angle_degrees * (PI / 180.0);
    let half_angle_rads = angle_rads / 2.0;

    let tan = half_angle_rads.tan();

    let base_depth = (base_diameter / 2.0) / tan;

    let full_radius = cut_diameter / 2.0;

    let full_depth = full_radius / tan;

    full_depth - base_depth
}
