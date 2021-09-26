use std::f32::consts::PI;
use std::time::Duration;

use common::errors::*;
use math::matrix::Vector3f;

use crate::color::*;
use crate::driver::*;

#[async_trait]
pub trait BlinkStickEffects {
    async fn transition(&self, c1: RGB, c2: RGB, duration: Duration) -> Result<()>;
}

#[async_trait]
impl BlinkStickEffects for BlinkStick {
    async fn transition(&self, c1: RGB, c2: RGB, duration: Duration) -> Result<()> {
        let start_time = std::time::Instant::now();

        let h1 = c1.to_hsv();
        let h2 = c2.to_hsv();

        loop {
            let now = std::time::Instant::now();

            let mut i = (now - start_time).as_secs_f32() / duration.as_secs_f32();
            if i > 1.0 {
                i = 1.0;
            }

            let hx = linear_interpolate_hsx(&h1, &h2, i);
            let rgb = RGB::from_hsv(&hx);

            self.set_first_color(rgb).await?;

            if i == 1.0 {
                break;
            }

            // Around 30 FPS.
            // TODO: Remove time spent on usb transaction.
            common::wait_for(Duration::from_millis(40)).await;
        }

        Ok(())
    }
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
