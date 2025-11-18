use core::fmt::Debug;

use crate::matrix::base::MatrixBase;
use crate::matrix::dimension::Dimension;
use crate::matrix::element::ElementType;
use crate::matrix::storage::StorageType;
use crate::number::AbsoluteValue;

impl<T: ElementType + Debug, R: Dimension, C: Dimension, D: StorageType<T, R, C>> Debug
    for MatrixBase<T, R, C, D>
{
    default fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                write!(f, "{:?}, ", self.data[i * self.cols() + j])?;
            }
            // write!(f, ", ")?;
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

                let va = AbsoluteValue::abs(v);
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

fn format_f32(f: &mut core::fmt::Formatter, v: f32) -> core::fmt::Result {
    let va = AbsoluteValue::abs(v);
    if va < 1e-12 {
        write!(f, "0.")?;
    } else if va > 1e9 || va < 1e-6 {
        write!(f, "{:+.4e}", v)?;
    } else {
        write!(f, "{}", format!("{:.4}", v).trim_end_matches('0'))?;

        // write!(f, "{:.4}", v)?; // TODO: Truncate zeros and
        //                             // decimal point.
    }

    Ok(())
}

impl<R: Dimension, C: Dimension, D: StorageType<f32, R, C>> Debug for MatrixBase<f32, R, C, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if self.rows() == 3 && self.cols() == 1 {
            write!(f, "vec3f(")?;
            format_f32(f, self.data[0]);
            write!(f, ", ")?;
            format_f32(f, self.data[1]);
            write!(f, ", ")?;
            format_f32(f, self.data[2]);
            write!(f, ")")?;
            return Ok(());
        }

        
        write!(f, "[")?;

        for i in 0..self.rows() {
            write!(f, "[")?;
            
            for j in 0..self.cols() {
                // TODO: If all numbers are very small, then don't truncate zeros.
                // TODO: Only format as exponential if the value is very small or
                // very large.

                let v = self.data[i * self.cols() + j];

                format_f32(f, v)?;

                let tab = if j == (self.cols() - 1) { "" } else { ",\t" };
                write!(f, "{}", tab)?;
            }

            write!(f, "]")?;

            if i != self.rows() - 1 {
                write!(f, ", ")?;
            }

            if self.rows() != 1 && self.cols() != 1 {
                write!(f, "\n")?;
            }
        }

        write!(f, "]")?;

        Ok(())
    }
}
