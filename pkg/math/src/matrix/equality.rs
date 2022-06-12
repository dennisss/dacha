use core::cmp::{Ordering, PartialOrd};

use approx::{AbsDiffEq, RelativeEq, UlpsEq};

use crate::matrix::base::MatrixBase;
use crate::matrix::dimension::Dimension;
use crate::matrix::element::ElementType;
use crate::matrix::storage::StorageType;

/// Equality of matrices is defined with exact matching of each element.
impl<T: PartialEq + ElementType, R: Dimension, C: Dimension, S: StorageType<T, R, C>> PartialEq
    for MatrixBase<T, R, C, S>
{
    fn eq(&self, other: &Self) -> bool {
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                if self[(i, j)] != other[(i, j)] {
                    return false;
                }
            }
        }

        true
    }
}

impl<T: PartialEq + ElementType, R: Dimension, C: Dimension, S: StorageType<T, R, C>> Eq
    for MatrixBase<T, R, C, S>
{
}

impl<T: PartialOrd + ElementType, R: Dimension, C: Dimension, S: StorageType<T, R, C>> PartialOrd
    for MatrixBase<T, R, C, S>
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let mut last_ordering = None;
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                let o = match self[(i, j)].partial_cmp(&other[(i, j)]) {
                    Some(o) => o,
                    None => return None,
                };

                if last_ordering.is_none() {
                    last_ordering = Some(o);
                } else if last_ordering != Some(o) {
                    return None;
                }
            }
        }

        last_ordering
    }
}

impl<T: AbsDiffEq + ElementType, R: Dimension, C: Dimension, S: StorageType<T, R, C>> AbsDiffEq
    for MatrixBase<T, R, C, S>
where
    T::Epsilon: Copy,
{
    type Epsilon = T::Epsilon;

    fn default_epsilon() -> T::Epsilon {
        T::default_epsilon()
    }

    fn abs_diff_eq(&self, other: &Self, epsilon: T::Epsilon) -> bool {
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                if !T::abs_diff_eq(&self[(i, j)], &other[(i, j)], epsilon) {
                    return false;
                }
            }
        }

        true
    }
}

impl<T: RelativeEq + ElementType, R: Dimension, C: Dimension, S: StorageType<T, R, C>> RelativeEq
    for MatrixBase<T, R, C, S>
where
    T::Epsilon: Copy,
{
    fn default_max_relative() -> T::Epsilon {
        T::default_max_relative()
    }

    fn relative_eq(&self, other: &Self, epsilon: T::Epsilon, max_relative: T::Epsilon) -> bool {
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                if !T::relative_eq(&self[(i, j)], &other[(i, j)], epsilon, max_relative) {
                    return false;
                }
            }
        }

        true
    }
}

impl<T: UlpsEq + ElementType, R: Dimension, C: Dimension, S: StorageType<T, R, C>> UlpsEq
    for MatrixBase<T, R, C, S>
where
    T::Epsilon: Copy,
{
    fn default_max_ulps() -> u32 {
        T::default_max_ulps()
    }

    fn ulps_eq(&self, other: &Self, epsilon: T::Epsilon, max_ulps: u32) -> bool {
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                if !T::ulps_eq(&self[(i, j)], &other[(i, j)], epsilon, max_ulps) {
                    return false;
                }
            }
        }

        true
    }
}
