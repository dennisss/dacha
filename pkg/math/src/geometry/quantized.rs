use core::cmp::Ordering;
use core::ops::Deref;

use crate::{
    matrix::{vec2f, Vector2, Vector2f},
    rational::Rational,
};

const SCALE: f32 = 1000.0;

/// Quantized vector which stores floating point values as integers 1000x the
/// size.
///
/// - PartialEq/Eq use exact comparison of the integer values.
/// - PartialOrd/Ord sort in standard line-sweep direction (y descending, then x
///   ascending).
///
/// TODO: Make the quantization scale configurable.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct QVector2f {
    inner: Vector2<i64>,
}

impl QVector2f {
    /// Returns a value in the range [0, 4] which increases monotonically with
    /// the clockwise angle of this vector from the +x axis.
    ///
    /// See https://stackoverflow.com/questions/16542042/fastest-way-to-sort-vectors-by-angle-without-actually-computing-that-angle
    pub fn pseudo_angle(&self) -> Rational {
        let dx = Rational::from(self.inner.x());
        let dy = Rational::from(self.inner.y());
        let one = Rational::from(1);
        let three = Rational::from(3);

        let p = dx / (dx.abs() + dy.abs());

        if self.inner.y() < 0 {
            three + p
        } else {
            one - p
        }
    }
}

/// TODO: Restrict usage of this?
impl Deref for QVector2f {
    type Target = Vector2<i64>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// TODO: Restrict usage of this.
impl From<Vector2<i64>> for QVector2f {
    fn from(inner: Vector2<i64>) -> Self {
        Self { inner }
    }
}

impl From<Vector2f> for QVector2f {
    fn from(v: Vector2f) -> Self {
        Self {
            inner: Vector2::from_slice(&[
                (v.x() * SCALE).round() as i64,
                (v.y() * SCALE).round() as i64,
            ]),
        }
    }
}

impl Into<Vector2f> for QVector2f {
    fn into(self) -> Vector2f {
        vec2f(
            (self.inner.x() as f32) / SCALE,
            (self.inner.y() as f32) / SCALE,
        )
    }
}

impl Ord for QVector2f {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.inner.y() == other.inner.y() {
            return self.inner.x().cmp(&other.inner.x());
        }

        other.inner.y().cmp(&self.inner.y())
    }
}

impl PartialOrd for QVector2f {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

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
