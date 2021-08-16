#[macro_use]
extern crate common;
extern crate blinkstick;
extern crate math;

use std::{f32::consts::PI, time::Duration};

use blinkstick::*;
use common::errors::*;
use math::matrix::Vector3f;

fn rgb_to_hsv_and_hsl(rgb: RGB) -> (Vector3f, Vector3f) {
    let rgb = Vector3f::from_slice(&[
        rgb.r as f32 / 255.0,
        rgb.g as f32 / 255.0,
        rgb.b as f32 / 255.0,
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

// TODO: Check this.
fn norm_radians(v: f32) -> f32 {
    let deg360 = 2.0 * PI;

    let mut m = v % deg360;
    if m < 0.0 {
        m += deg360;
    }

    assert!(m >= 0.0 && m < deg360);

    m
}

fn hsv_to_rgb(hsv: Vector3f) -> Vector3f {
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

fn linear_interpolate_hsx(a: &Vector3f, b: &Vector3f, i: f32) -> Vector3f {
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

async fn transition(blink: &BlinkStick, c1: RGB, c2: RGB, duration: Duration) -> Result<()> {
    let start_time = std::time::Instant::now();
    let end_time = start_time + duration;

    let h1 = rgb_to_hsv_and_hsl(c1).0;
    let h2 = rgb_to_hsv_and_hsl(c2).0;

    loop {
        let now = std::time::Instant::now();

        let mut i = (now - start_time).as_secs_f32() / duration.as_secs_f32();
        if i > 1.0 {
            i = 1.0;
        }

        let hx = linear_interpolate_hsx(&h1, &h2, i);
        let rgb = hsv_to_rgb(hx);

        blink
            .set_first_color(RGB {
                r: (rgb[0] * 255.0).round() as u8,
                g: (rgb[1] * 255.0).round() as u8,
                b: (rgb[2] * 255.0).round() as u8,
            })
            .await?;

        if i == 1.0 {
            break;
        }

        // Around 30 FPS.
        // TODO: Remove time spent on usb transaction.
        common::wait_for(Duration::from_millis(40)).await;
    }

    Ok(())
}

async fn read_controller() -> Result<()> {
    println!("{}", -5.0 % 3.0);
    println!("{}", 5.0 % 3.0);
    println!("{}", 1.0 % 3.0);

    let blink = BlinkStick::open().await?;

    let x = 50;
    let c1 = RGB { r: 0, g: 0, b: x };
    let c2 = RGB { r: 0, g: x, b: 0 };
    let c3 = RGB { r: x, g: 0, b: 0 };

    for i in 0..10 {
        transition(&blink, c1, c2, Duration::from_millis(2000)).await?;
        transition(&blink, c2, c3, Duration::from_millis(2000)).await?;
        transition(&blink, c3, c1, Duration::from_millis(2000)).await?;

        /*
        blink.set_colors(0, &[ c1, c2 ]).await?;

        common::wait_for(Duration::from_millis(500)).await;

        blink.set_colors(0, &[ c2, c1 ]).await?;

        common::wait_for(Duration::from_millis(500)).await;
        */
    }

    blink.turn_off().await?;

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(read_controller())
}
