use core::ops::Deref;
use core::{cmp::Ordering, ops::Sub};

use crate::matrix::element::{FloatElementType, ScalarElementType};
use crate::matrix::Vector2i64;
use crate::number::Cast;
use crate::number::Float;
use crate::{
    matrix::{vec2f, Vector2, Vector2f},
    rational::Rational,
};

pub const DEFAULT_SCALE: f32 = 4096.0;

// TODO: Ensure this is never used on integer types since division will be very
// lossy.
pub trait PseudoAngle {
    type Output;

    /// Returns a value in the range [0, 4] which increases monotonically with
    /// the counter-clockwise angle of this vector from the +x axis.
    ///
    /// The [0, 4] range roughly corresponds to the range [0, 2*pi] radians.
    ///
    /// Some known angles:
    /// - '1' will be 'pi/2' radians
    /// - '2' will be 'pi' radians
    /// - '3' will be '3*pi/2' radians
    ///
    /// See https://stackoverflow.com/questions/16542042/fastest-way-to-sort-vectors-by-angle-without-actually-computing-that-angle
    fn pseudo_angle(&self) -> Self::Output;
}

impl<T: ScalarElementType> PseudoAngle for Vector2<T> {
    type Output = T;

    fn pseudo_angle(&self) -> Self::Output {
        let p = self.x() / (self.x().abs() + self.y().abs());
        if self.y() < T::zero() {
            T::from(3) + p
        } else {
            T::from(1) - p
        }
    }
}

pub fn quantize2<T: FloatElementType>(v: Vector2<T>, scale: f32) -> Vector2i64 {
    Vector2::from_slice(&[
        (v.x() * T::from(scale)).round().cast(),
        (v.y() * T::from(scale)).round().cast(),
    ])
}

pub fn dequantize2<T: FloatElementType>(v: Vector2i64, scale: f32) -> Vector2<T> {
    Vector2::from_slice(&[
        Cast::<T>::cast(v.x()) / T::from(scale),
        Cast::<T>::cast(v.y()) / T::from(scale),
    ])
}

#[cfg(test)]
mod tests {
    use crate::{geometry::quantized::PseudoAngle, matrix::vec2f};

    #[test]
    fn pseudo_angle_test() {
        println!("{}", vec2f(1.0, 0.0).pseudo_angle());
        println!("{}", vec2f(1.0, 0.1).pseudo_angle());
        println!("{}", vec2f(1.0, 1.0).pseudo_angle());
        println!("{}", vec2f(0.0, 1.0).pseudo_angle());
        println!("{}", vec2f(1.0, -0.1).pseudo_angle());
    }
}
