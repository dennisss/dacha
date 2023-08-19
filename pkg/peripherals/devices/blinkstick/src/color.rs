use std::f32::consts::PI;

use math::matrix::Vector3f;

#[derive(Clone, Copy)]
pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RGB {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_hsv(&self) -> Vector3f {
        self.to_hsv_and_hsl().0
    }

    pub fn to_hsl(&self) -> Vector3f {
        self.to_hsv_and_hsl().1
    }

    fn to_hsv_and_hsl(&self) -> (Vector3f, Vector3f) {
        let rgb = Vector3f::from_slice(&[
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        ]);

        let x_max = rgb.max_value();
        let x_min = rgb.min_value();

        let v = x_max;
        let range = x_max - x_min;
        let l = v - (range / 2.0); // lightness

        // 60 degrees in radians.
        let deg60 = 60.0 * (PI / 180.0);

        let hue = deg60 * {
            if range.abs() < 1e-6 {
                0.0
            } else if v == rgb[0] {
                // v == r
                0.0 + (rgb[1] - rgb[2]) / range
            } else if v == rgb[1] {
                // v == g
                2.0 + (rgb[2] - rgb[0]) / range
            } else {
                // v == b
                4.0 + (rgb[0] - rgb[1]) / range
            }
        };

        let s_v = {
            // TODO: Use approximate eq
            if v.abs() < 1e-6 {
                0.0
            } else {
                range / v
            }
        };

        let s_l = {
            // TODO: Use approximate eq
            if l.abs() < 1e-6 || (l - 1.0).abs() < 1e-6 {
                0.0
            } else {
                (v - l) / f32::min(l, 1.0 - l)
            }
        };

        (
            Vector3f::from_slice(&[hue, s_v, v]),
            Vector3f::from_slice(&[hue, s_l, l]),
        )
    }

    pub fn from_hsv(hsv: &Vector3f) -> Self {
        let rgb = Self::from_hsv_float(hsv);
        Self {
            r: (rgb[0] * 255.0).round() as u8,
            g: (rgb[1] * 255.0).round() as u8,
            b: (rgb[2] * 255.0).round() as u8,
        }
    }

    fn from_hsv_float(hsv: &Vector3f) -> Vector3f {
        let h: f32 = hsv[0];
        let s: f32 = hsv[1];
        let v: f32 = hsv[2];

        let c = v * s;

        let deg60 = 60.0 * (PI / 180.0);
        let h2 = h / deg60;

        let x = c * (1.0 - ((h2 % 2.0) - 1.0).abs());

        let (r1, g1, b1) = {
            if !h.is_finite() {
                (0.0, 0.0, 0.0)
            } else if h <= 1.0 {
                (c, x, 0.0)
            } else if h <= 2.0 {
                (x, c, 0.0)
            } else if h <= 3.0 {
                (0.0, c, x)
            } else if h <= 4.0 {
                (0.0, x, c)
            } else if h <= 5.0 {
                (x, 0.0, c)
            } else {
                // <= 6
                (c, 0.0, x)
            }
        };

        let m = v - c;

        Vector3f::from_slice(&[r1, g1, b1]) + m
    }
}
