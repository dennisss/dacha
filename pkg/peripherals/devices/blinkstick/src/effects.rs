use std::f32::consts::PI;
use std::time::Duration;

use common::errors::*;
use math::matrix::Vector3f;
use color::*;

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
            executor::sleep(Duration::from_millis(40)).await;
        }

        Ok(())
    }
}

