use alloc::vec::Vec;

use crate::matrix::element::{ErrorEpsilon, FloatElementType, ScalarElementType};
use crate::matrix::{Matrix2, Vector2};

use super::line_segment::LineSegment2;

/// Representation of an unbounded 2d line where a point is defined as:
/// p = base + (lambda * dir)
#[derive(Clone, Debug)]
pub struct Line2<T: ScalarElementType> {
    pub base: Vector2<T>,
    pub dir: Vector2<T>,
}

impl<T: ScalarElementType + ErrorEpsilon> Line2<T> {
    /// NOTE: When using this formulation, it is guaranteed that base will equal
    /// x1 and (base + dir) will equal x2. So this can be used to recover the
    /// original line segment. But all other operations still assume that the
    /// line is continuous.
    pub fn from_points(x1: &Vector2<T>, x2: &Vector2<T>) -> Self {
        Self {
            base: (*x1).clone(),
            dir: x2 - x1,
        }
    }

    /// Returns a vector which is pointing in a perpendicular direction (left)
    /// to this line (when starting at the same base point).
    pub fn perp(&self) -> Vector2<T> {
        Vector2::from_slice(&[T::from(-1) * self.dir.y(), self.dir.x()])
    }

    /// Given that self is: p = base1 + (lambda1 * dir1)
    /// and other is:       p = base2 + (lambda2 * dir2)
    ///
    /// Then the intersection is:
    ///   base1 + (lambda1 * dir1) = base2 + (lambda2 * dir2)
    ///   (lambda1 * dir1) - (lambda2 * dir2) = base2 - base1
    pub fn intersect(&self, other: &Self) -> Option<Vector2<T>> {
        let x = match self.intersection_coeff_unchecked(other) {
            Some(v) => v,
            None => return None,
        };

        Some(self.evaluate(x[0]))
    }

    /// Computes the intersection of two lines which which represent closed
    /// line segments.
    ///
    /// NOTE: This only returns 'exact' intersections (no intersections that
    /// exceed the end points of the segments).
    ///
    /// TODO: Dedup with the other one.
    pub fn intersect_segments_exact(&self, other: &Self) -> Option<Vector2<T>> {
        let x = match self.intersection_coeff_unchecked(other) {
            Some(v) => v,
            None => return None,
        };

        // NOTE: No error tolerance here so only suitable for exact arithmetic.
        if x[0] < T::zero() || x[0] > T::one() || x[1] < T::zero() || x[1] > T::one() {
            return None;
        }

        Some(self.evaluate(x[0]))
    }

    /// Computes the raw line coefficients (the lambda part in 'p = base +
    /// (lambda * dir)') at which 'self' and 'other' maybe intersect.
    ///
    /// This will only return None if the lines are parallel.
    pub fn intersection_coeff_unchecked(&self, other: &Self) -> Option<Vector2<T>> {
        let mut A = Matrix2::zero();
        A.block_mut(0, 0).copy_from(&self.dir);
        A.block_mut(0, 1).copy_from(&other.dir);

        let b = &other.base - &self.base;

        let A_inv = match A.checked_inverse_2x2() {
            Some(x) => x,
            None => return None,
        };

        let mut x = A_inv * b;
        x[1] *= -T::one();

        Some(x)
    }

    pub fn standard_form_coeffs(&self) -> (T, T, T) {
        // (x * dy) - (y * d_x) =  (b_x * dy) - (b_y * d_x)

        let a = self.dir.y();
        let b = T::from(-1) * self.dir.x();
        let c = (self.base.x() * self.dir.y()) - (self.base.y() * self.dir.x());
        (a, b, c)
    }

    pub fn evaluate(&self, t: T) -> Vector2<T> {
        &self.base + (self.dir.to_owned() * t)
    }
}

impl<T: FloatElementType> Line2<T> {
    pub fn distance_to_point(&self, point: &Vector2<T>) -> T {
        let dir_perp = self.perp().normalized();
        let offset = point - &self.base;
        dir_perp.dot(&offset)
    }
}
