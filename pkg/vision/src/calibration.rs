use math::matrix::{Vector2d, Vector3d, Matrix3d};

use crate::camera::*;
use crate::bundle::*;
use crate::pnp::*;
use crate::homography::*;

pub struct CameraInstrinsicsSolver {
    initial_intrinsics: CameraIntrinsicsModel,
    objects: Vec<Object>,
}

struct Object {
    points_3d: Vec<Vector3d>,
    points_2d: Vec<Vector2d>,
    homography: Matrix3d,
}

#[derive(Debug, Clone)]
pub struct CameraIntrinsicsSolution {
    pub intrinsics: CameraIntrinsicsModel,
    pub error: f64,
}

impl CameraInstrinsicsSolver {
    pub fn new(initial_intrinsics: &CameraIntrinsicsModel) -> Self {
        Self {
            initial_intrinsics: initial_intrinsics.clone(),
            objects: vec![],
        }
    }

    pub fn add_object(
        &mut self,
        points_3d: &[Vector3d],
        points_2d: &[Vector2d],
    ) {
        let homography = find_camera_homography(
            &self.initial_intrinsics,
            points_3d,
            points_2d
        );

        self.objects.push(Object {
            points_3d: points_3d.into(),
            points_2d: points_2d.into(),
            homography
        });
    }

    pub fn solve(&self) -> CameraIntrinsicsSolution {
        // Optionally init focal length and center from homography.
        // TODO: Need to scale by current focal length / center
        /*
        let mut homographies = vec![];
        for obj in &objects {
            homographies.push(obj.homography.clone());
        }

        let (f, c) = intrinsics_from_homographies(&homographies);

        println!("F: {:?}", f);
        println!("C: {:?}", c);
        */

        let mut bundle = BundleAdjustmentSolver::new();
        bundle.enable_logging();

        let cam_i = bundle.add_camera(
            &self.initial_intrinsics,
            &Vector3d::zero(),
            &Vector3d::zero(),
            true // fixed
        );

        for obj in &self.objects {
            let mut pnp_solver = PnPSolver::new(
                &self.initial_intrinsics, &obj.points_2d, &obj.points_3d
            );
            pnp_solver.set_initial_extrinsics_from_homography(&obj.homography);

            let pnp_res = pnp_solver.solve();
            println!("PNP RMS: {:?}", pnp_res.error);

            let obj_i = bundle.add_object(
                &pnp_res.rotation,
                &pnp_res.translation,
            );

            for (pt3, pt2) in obj.points_3d.iter().zip(obj.points_2d.iter()) {
                bundle.add_object_point_view(
                    obj_i,
                    cam_i,
                    pt2,
                    pt3,
                );
            }
        }

        let solution = bundle.solve();

        CameraIntrinsicsSolution {
            error: solution.error(),
            intrinsics: solution.camera_intrinsics(cam_i)
        }
    }

}