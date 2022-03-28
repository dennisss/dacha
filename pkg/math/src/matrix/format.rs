use core::fmt::Debug;

use num_traits::real::Real;

use crate::matrix::base::MatrixBase;
use crate::matrix::dimension::Dimension;
use crate::matrix::element::ElementType;
use crate::matrix::storage::StorageType;

impl<T: ElementType + Debug, R: Dimension, C: Dimension, D: StorageType<T, R, C>> Debug
    for MatrixBase<T, R, C, D>
{
    default fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                write!(f, "{:?}\t", self.data[i * self.cols() + j])?;
            }
            write!(f, "\n")?;
        }

        Ok(())
    }
}

// TODO: Also do this for f32
impl<R: Dimension, C: Dimension, D: StorageType<f64, R, C>> Debug for MatrixBase<f64, R, C, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                // TODO: If all numbers are very small, then don't truncate zeros.
                // TODO: Only format as exponential if the value is very small or
                // very large.

                let v = self.data[i * self.cols() + j];

                let va = Real::abs(v);
                if va < 1e-12 {
                    write!(f, "0\t")?;
                } else if va > 1e9 || va < 1e-6 {
                    write!(f, "{:+.4e}\t", v)?;
                } else {
                    write!(f, "{:.4}\t", v)?; // TODO: Truncate zeros and
                                              // decimal point.
                }
            }
            write!(f, "\n")?;
        }

        Ok(())
    }
}
