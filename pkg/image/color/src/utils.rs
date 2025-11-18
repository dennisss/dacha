use std::f32::consts::PI;

use math::matrix::Vector3f;

fn norm_radians(v: f32) -> f32 {
    let deg360 = 2.0 * PI;

    let mut m = v % deg360;
    if m < 0.0 {
        m += deg360;
    }

    assert!(m >= 0.0 && m < deg360);

    m
}

pub fn linear_interpolate_hsx(a: &Vector3f, b: &Vector3f, i: f32) -> Vector3f {
    let deg180 = PI;
    let deg360 = 2.0 * PI;

    let mut hue_distance = norm_radians(b[0] - a[0]);
    if hue_distance > deg180 {
        hue_distance = -1.0 * norm_radians(a[0] - b[0]);
        // hue_distance -= deg360;
    };

    let hue = norm_radians(a[0] + i * hue_distance);

    let s = a[1] * (1.0 - i) + b[1] * i;
    let x = a[2] * (1.0 - i) + b[2] * i;

    Vector3f::from_slice(&[hue, s, x])
}