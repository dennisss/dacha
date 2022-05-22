use image::{Color, Image};
use math::matrix::Vector2i;

use crate::raster::utils::*;

/// Draws a line between two integer points.
/// TODO: Should appropriately mix alphas
pub fn bresenham_line(image: &mut Image<u8>, start: Vector2i, end: Vector2i, color: &Color) {
    let dx = end.x() - start.x();
    let dy = end.y() - start.y();

    if dx.abs() >= dy.abs() {
        let derror = ((dy as f32) / (dx as f32)).abs();
        let mut error = 0.0;

        let mut y = start.y();
        for x in closed_range(start.x(), end.x()) {
            image.set(y as usize, x as usize, color);

            error += derror;
            if error >= 0.5 {
                y += dy.signum();
                error -= 1.0;
            }
        }
    } else {
        let derror = ((dx as f32) / (dy as f32)).abs();
        let mut error = 0.0;
        let mut x = start.x();
        for y in closed_range(start.y(), end.y()) {
            image.set(y as usize, x as usize, color);

            error += derror;
            if error >= 0.5 {
                x += dx.signum();
                error -= 1.0;
            }
        }
    }
}
