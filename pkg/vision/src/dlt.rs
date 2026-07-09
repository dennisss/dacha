use math::matrix::{MatrixX4d, Vector2d, Vector3d, vec3d};
use math::matrix::svd::SVD;
use typenum::{U1, U3, U4};

use crate::extrinsics::*;

/// Triangulates >= 2 points.
pub struct DLTSolver {
    mat: MatrixX4d,
    i: usize,
}

impl DLTSolver {
    pub fn new(num_views: usize) -> Self {
        Self {
            mat: MatrixX4d::zero_with_shape(num_views * 2, 4),
            i: 0,
        }
    }

    pub fn add_normalized_view(&mut self, extrinsics: &CameraExtrinsics, point: &Vector2d) {
        let p = extrinsics.to_mat4x4();

        self.mat.row_mut(2*self.i).copy_from(
            &(p.row(2).to_owned() * point[0] - p.row(0))
        );
        self.mat.row_mut(2*self.i + 1).copy_from(
            &(p.row(2).to_owned() * point[1] - p.row(1))
        );

        self.i += 1;
    }

    #[inline(never)]
    pub fn solve(&self) -> Option<Vector3d> {
        assert_eq!(2 * self.i, self.mat.rows());

        // Fast 2 view case.
        if self.i == 2 {
            let m = self.mat.block::<U4, U3>(0, 0).to_owned();
            let b = self.mat.block::<U4, U1>(0, 3);

            let x = match (m.transpose() * &m).inverse() {
                Some(v) => v,
                None => return None
            };

            let x = x * m.transpose();

            let x = x * b;
            return Some(x * -1.0);
        }


        let mut svd = SVD::eigen_svd(&self.mat);

        // This is 4x1
        let p = svd.v.col(svd.v.cols() - 1);
        
        let w = p[3];
        if w.abs() < 1e-6 {
            return None;
        }

        Some(vec3d(
            p[0] / w,
            p[1] / w,
            p[2] / w
        ))
    }
}


