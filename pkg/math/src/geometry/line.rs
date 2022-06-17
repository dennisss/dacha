use alloc::vec::Vec;

use crate::matrix::element::FloatElementType;
use crate::matrix::{Matrix2, Vector2};

/// Representation of an unbounded 2d line where a point is defined as:
/// p = base + (lambda * dir)
#[derive(Clone)]
pub struct Line2<T: FloatElementType> {
    pub base: Vector2<T>,
    pub dir: Vector2<T>,
}

impl<T: FloatElementType> Line2<T> {
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

    pub fn distance_to_point(&self, point: &Vector2<T>) -> T {
        let dir_perp =
            Vector2::from_slice(&[T::from(-1.) * self.dir.y(), self.dir.x()]).normalized();
        let offset = point - &self.base;
        dir_perp.dot(&offset)
    }

    /// Given that self is: p = base1 + (lambda1 * dir1)
    /// and other is:       p = base2 + (lambda2 * dir2)
    ///
    /// Then the intersection is:
    ///   base1 + (lambda1 * dir1) = base2 + (lambda2 * dir2)
    ///   (lambda1 * dir1) - (lambda2 * dir2) = base2 - base1
    pub fn intersect(&self, other: &Self) -> Option<Vector2<T>> {
        let mut A = Matrix2::zero();
        A.block_mut(0, 0).copy_from(&self.dir);
        A.block_mut(0, 1).copy_from(&other.dir);

        let b = &other.base - &self.base;

        if A.determinant().abs() < T::from(1e-6) {
            None
        } else {
            let x = A.inverse() * b;
            Some(self.evaluate(x[0]))
        }
    }

    pub fn standard_form_coeffs(&self) -> (T, T, T) {
        // (x * dy) - (y * d_x) =  (b_x * dy) - (b_y * d_x)

        let a = self.dir.y();
        let b = T::from(-1.) * self.dir.x();
        let c = (self.base.x() * self.dir.y()) - (self.base.y() * self.dir.x());
        (a, b, c)
    }

    pub fn evaluate(&self, t: T) -> Vector2<T> {
        &self.base + (self.dir.to_owned() * t)
    }
}
