use math::matrix::{Vector3d, VectorXd, MatrixXd};
use vision::solver::*;

use crate::skeleton::inst::*;
use crate::skeleton::tree::*;


// TODO: Ideally require some minimum error and confidence to consider the match to be successful.
pub fn solve_skeleton_joints_state(
    skeleton: &Skeleton,
    skeleton_tree: &SkeletonTree,
    initial_state: &SkeletonJointsState,
    markers: &[(usize, Vector3d)]
) -> SkeletonJointsState {
    let mut solver = NonLinearSolver::new();
    solver.set_max_iterations(100);
    // solver.set_min_error(0.0001 * ((markers.len() * 3) as f64));
    // solver.enable_logging();

    let param = solver.add_parameter_block(
        &initial_state.serialize(skeleton),
        SkeletonJointStatesParameterBlock { skeleton }
    );

    solver.add_residual_block(&[ param ], SkeletonMarkerResidual {
        skeleton,
        skeleton_tree,
        target_positions: markers
    });

    let solution = solver.solve();

    SkeletonJointsState::parse(&solution.params, skeleton)
}


struct SkeletonJointStatesParameterBlock<'a> {
    skeleton: &'a Skeleton,
}

impl<'a> ParameterBlockOperator for SkeletonJointStatesParameterBlock<'a> {
    fn update(&self, step: &[f64], params: &mut [f64]) {        
        for i in 0..step.len() {
            params[i] += step[i];
        }

        let mut s = SkeletonJointsState::parse(params, self.skeleton);
        s.clamp(self.skeleton);
        params.copy_from_slice(&s.serialize(self.skeleton));
    }
}


/// Computes the positional error of markers attached to a skeleton
/// given a specific configuration of joints.
struct SkeletonMarkerResidual<'a> {
    skeleton: &'a Skeleton,
    skeleton_tree: &'a SkeletonTree,
    target_positions: &'a [(usize, Vector3d)]
}

impl<'a> SkeletonMarkerResidual<'a> {
    fn calc_error(&self, params: &[f64], marker_positions: &mut [Vector3d], out: &mut MatrixXd) {
        let state = SkeletonJointsState::parse(params, &self.skeleton);

        // TODO: Need small angle representations for everything
        self.skeleton_tree.forward_kinematics(&state, &mut [], marker_positions);

        for (i, (marker_i, target_position)) in self.target_positions.iter().enumerate() {
            let e = target_position - &marker_positions[*marker_i];
            out.row_mut(i).copy_from_slice(e.as_ref());
        }
    }
}

impl<'a> ResidualBlockFunction for SkeletonMarkerResidual<'a> {

    fn len(&self) -> usize {
        self.target_positions.len() * 3
    }


    fn calculate(&self, params: &[f64], out: &mut [f64], gradient: &mut [f64]) {
        // Shared buffers.
        let mut marker_positions = vec![Vector3d::zero(); self.skeleton.markers.len()];
        let mut error1 = MatrixXd::zero_with_shape(self.target_positions.len(), 3);
        let mut error2 = MatrixXd::zero_with_shape(self.target_positions.len(), 3);

        let num_residuals = self.len();

        self.calc_error(params, &mut marker_positions, &mut error1);
        out.copy_from_slice(error1.as_ref());

        let mut params = params.to_vec();

        let step = 0.00001;
        let scaler = 1.0 / (-2.0 * step);

        for i in 0..params.len() {
            let v = params[i];
            params[i]  = v + step;

            self.calc_error(&params, &mut marker_positions, &mut error1);

            params[i]  = v - step;
            self.calc_error(&params, &mut marker_positions, &mut error2);

            params[i] = v;

            // iter over # residuals
            for j in 0..num_residuals {
                gradient[j*params.len() + i] = (error1[j] - error2[j]) * scaler
            }
        }
    }

}

