use math::matrix::{VectorXf, MatrixXf, Vector3f, Matrix3f, Vector2f, vec2f, vec3f};


pub trait NonLinearProblem {
    fn num_points(&self) -> usize;

    fn error(&self, point_idx: usize, params: &[f32], gradient: &mut [f32]) -> f32;
    
    fn update(&self, step: &VectorXf, params: &mut VectorXf);
}

/// 
const MIN_IMPROVEMENT_PERCENTAGE: f32 = 0.001;  // 0.1%

const MAX_ITERATIONS: usize = 10_000;

/// Once the error goes below this threshold, we will just stop.
const MIN_ERROR: f32 = 0.000001;


#[derive(Clone, Debug)]
pub struct NonLinearProblemSolution {
    pub params: Vec<f32>,
    pub error_sum: f32    
}

/// Levenberg-Marquardt non-linear least squares solver
pub fn solve_nonlinear(initial_params: &[f32], problem: &dyn NonLinearProblem) -> NonLinearProblemSolution {
    // Current value of each parameter in the model that we are estimating.
    let mut params = VectorXf::from_slice_with_shape(initial_params.len(), 1, initial_params);
    
    // Last parameter set and it's overall error sum.
    let mut last_params: Option<(VectorXf, f32)> = None;

    // Each element is the error for a single point with the current parameters.
    let mut error = VectorXf::zero_with_shape(problem.num_points(), 1);

    let mut error_sum = 0.0;

    // Each row is the gradients of each parameter for a single point.
    let mut jacobian = MatrixXf::zero_with_shape(problem.num_points(), initial_params.len());

    let mut dampening: f32 = 0.01;

    let mut dampening_is_clamped = false;
    let mut last_made_progress = true; 

    // Normally this will terminate by:
    // - First there will likely be many rounds of last_made_progress=true
    // - Once the error hits the minimum, we will try ramping the dampening and
    //   the clamping of the dampening to a max value will signal that we hit the
    //   min and can't find a way to make more progress. 
    let mut iters = 0;
    while iters < MAX_ITERATIONS && (last_made_progress || !dampening_is_clamped) {
        iters += 1;
        
        error_sum = 0.0;
        for i in 0..problem.num_points() {
            let e = problem.error(
                i,
                params.as_ref(),
                &mut jacobian.as_mut()[ (i * params.len())..((i+1) * params.len())]
            );
            
            error_sum += e;
            error[i] = e;
        }

        if error_sum <= MIN_ERROR {
            break;
        }

        if let Some((last_params, last_error_sum)) = last_params.take() {            
            if error_sum.is_nan() || ((error_sum - last_error_sum) / last_error_sum) >= -MIN_IMPROVEMENT_PERCENTAGE {
                // TODO: If we end up running this, then we will end up recalculating jacobians on the next iteration 
                params = last_params;
                error_sum = last_error_sum;
                dampening *= 10.0;

                last_made_progress = false;
                dampening_is_clamped = false;
                continue;
            }

            dampening /= 2.0;
            last_made_progress = true;
        }

        last_params = Some((params.clone(), error_sum));

        {
            let clamped_dampening = dampening.min(10_000_000.0).max(0.0000001);
            dampening_is_clamped = clamped_dampening != dampening;
            dampening = clamped_dampening;
        }

        let jacobian_square = jacobian.transpose() * &jacobian;

        let dampening_mat = diag(&jacobian_square) * dampening;

        let a = jacobian_square + dampening_mat;

        let a_inv = a.inverse();

        let step = a_inv * jacobian.transpose() * &error;

        problem.update(&step, &mut params);
    }

    if let Some((last_params, last_error_sum)) = last_params.take() {
        if last_error_sum < error_sum {
            params = last_params;
            error_sum = last_error_sum;
        }
    }

    NonLinearProblemSolution {
        params: params.as_ref().to_vec(),
        error_sum
    }    
}

fn diag(mat: &MatrixXf) -> MatrixXf {
    let mut out = MatrixXf::zero_with_shape(mat.rows(), mat.cols());
    for i in 0..mat.rows() {
        out[(i, i)] = mat[(i, i)]
    }
    out
}

#[cfg(test)]
mod tests {

    use super::*;

    // Model is 'y = Ax^2 + B'
    struct QuadraticProblem {
        // These are hidden from the solver and we expect the solver to be able to find them
        // just based on the data.
        a: f32,
        b: f32,
        x: Vec<f32>,
    }

    impl NonLinearProblem for QuadraticProblem {
        fn num_points(&self) -> usize {
            self.x.len()
        }

        fn error(&self, point_idx: usize, params: &[f32], gradient: &mut [f32]) -> f32 {
            let x = self.x[point_idx];
            let expected_y = self.a * x + self.b;

            let y = params[0] * x + params[1];
            let error = expected_y - y;

            gradient[0] = x;
            gradient[1] = 1.0;

            error
        }

        fn update(&self, step: &VectorXf, params: &mut VectorXf) {
            *params += step;
        }
    }


    #[test]
    fn solve_nonlinear_works() {

        let problem = QuadraticProblem {
            a: 1.0,
            b: 2.0,
            x: vec![-2.0, -1.0, 0.0, 1.0, 2.0]
        };

        let solution = solve_nonlinear(&[0.0, 0.0], &problem);
        assert_eq!(solution.params, &[1.0, 2.0]);
    }

}
