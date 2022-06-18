use core::ops::Deref;
use core::{cmp::Ordering, ops::Sub};

use crate::matrix::element::FloatElementType;
use crate::matrix::Vector2i64;
use crate::number::Cast;
use crate::number::Float;
use crate::{
    matrix::{vec2f, Vector2, Vector2f},
    rational::Rational,
};

const SCALE: f32 = 1000.0;

// /// Quantized vector which stores floating point values as integers 1000x the
// /// size.
// ///
// /// - PartialEq/Eq use exact comparison of the integer values.
// /// - PartialOrd/Ord sort in standard line-sweep direction (y descending,
// then x ///   ascending).
// ///
// /// TODO: Make the quantization scale configurable.
// #[derive(Clone, Debug, PartialEq, Eq)]
// #[repr(transparent)]
// pub struct QVector2f {
//     inner: Vector2<i64>,
// }

pub trait PseudoAngle {
    type Output;

    /// Returns a value in the range [0, 4] which increases monotonically with
    /// the clockwise angle of this vector from the +x axis.
    ///
    /// The [0, 4] range roughly corresponds to the range [0, 2*pi] radians.
    ///
    /// See https://stackoverflow.com/questions/16542042/fastest-way-to-sort-vectors-by-angle-without-actually-computing-that-angle
    fn pseudo_angle(&self) -> Self::Output;
}

impl PseudoAngle for Vector2i64 {
    type Output = Rational;

    fn pseudo_angle(&self) -> Rational {
        let dx = Rational::from(self.x());
        let dy = Rational::from(self.y());
        let one = Rational::from(1);
        let three = Rational::from(3);

        let p = dx / (dx.abs() + dy.abs());

        if self.y() < 0 {
            three + p
        } else {
            one - p
        }
    }
}

impl<T: FloatElementType> PseudoAngle for Vector2<T> {
    type Output = T;

    fn pseudo_angle(&self) -> Self::Output {
        let p = self.x() / (self.x().abs() + self.y().abs());
        if self.y() < T::zero() {
            T::from(3i8) + p
        } else {
            T::from(1i8) - p
        }
    }
}

pub fn quantize2<T: FloatElementType>(v: Vector2<T>) -> Vector2i64 {
    Vector2::from_slice(&[
        (v.x() * T::from(SCALE)).round().cast(),
        (v.y() * T::from(SCALE)).round().cast(),
    ])
}

pub fn dequantize2<T: FloatElementType>(v: Vector2i64) -> Vector2<T> {
    Vector2::from_slice(&[
        Cast::<T>::cast(v.x()) / T::from(SCALE),
        Cast::<T>::cast(v.y()) / T::from(SCALE),
    ])
}

/*
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {
        let vecs = &[
            QVector2f::from(vec2f(1., 0.0)),
            QVector2f::from(vec2f(1., 0.1)),
            QVector2f::from(vec2f(1., 1.)),
            QVector2f::from(vec2f(0.2, 0.9)),
            QVector2f::from(vec2f(0., 1.)),
            QVector2f::from(vec2f(-1., 0.5)),
            QVector2f::from(vec2f(-1., 0.)),
            QVector2f::from(vec2f(-1., -0.2)),
            QVector2f::from(vec2f(0., -0.5)),
            QVector2f::from(vec2f(0.7, -0.5)),
        ];

        // for i in 0..vecs.len() {
        //     println!("{:?}  => {:?}", vecs[i], vecs[i].pseudo_angle().to_f32());
        // }
        // println!("====");

        for i in 0..(vecs.len() - 1) {
            assert!(vecs[i].pseudo_angle() < vecs[i + 1].pseudo_angle());
        }
    }
}
*/
