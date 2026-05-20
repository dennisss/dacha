use math::matrix::{MatrixX4f, Vector2f, Vector3f, vec3f};
use math::matrix::svd::SVD;

use crate::extrinsics::*;

/// Triangulates >= 2 
pub struct DLTSolver {
    mat: MatrixX4f,
    i: usize,
}

impl DLTSolver {
    pub fn new(num_views: usize) -> Self {
        Self {
            mat: MatrixX4f::zero_with_shape(num_views * 2, 4),
            i: 0,
        }
    }

    pub fn add_normalized_view(&mut self, extrinsics: &CameraExtrinsics, point: &Vector2f) {
        let p = extrinsics.to_mat4x4();

        self.mat.row_mut(2*self.i).copy_from(
            &(p.row(2).to_owned() * point[0] - p.row(0))
        );
        self.mat.row_mut(2*self.i + 1).copy_from(
            &(p.row(2).to_owned() * point[1] - p.row(1))
        );

        self.i += 1;
    }

    pub fn solve(&self) -> Vector3f {
        assert_eq!(2 * self.i, self.mat.rows());

        let mut svd = SVD::eigen_svd(&self.mat);

        // This is 4x1
        let p = svd.v.col(svd.v.cols() - 1);
        
        let w = p[3];

        vec3f(
            p[0] / w,
            p[1] / w,
            p[2] / w
        )
    }
}


