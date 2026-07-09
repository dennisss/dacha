pub mod base;
pub mod cwise_binary_ops;
pub mod dimension;
#[cfg(feature = "alloc")]
pub mod eigen;
pub mod element;
pub mod equality;
pub mod format;
mod helpers;
#[cfg(feature = "alloc")]
pub mod householder;
pub mod multiplication;
#[cfg(feature = "alloc")]
pub mod qr;
pub mod storage;
#[cfg(feature = "alloc")]
pub mod svd;
#[cfg(feature = "alloc")]
pub mod axis_angle;

pub use self::base::*;
pub use self::dimension::*;
pub use self::helpers::*;


#[cfg(feature = "alloc")]
pub fn pinv(x: &MatrixXd) -> MatrixXd {
    x.transpose() * (x * x.transpose()).inverse().unwrap()
}
