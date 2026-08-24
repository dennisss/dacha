use std::time::Instant;

use math::matrix::{VectorXd, MatrixXd, Vector3d, Matrix3d, Vector2d, vec2d, vec3d};
use math::matrix::axis_angle::*;
use math::matrix::cwise_binary_ops::CwiseMulAssign;

const MIN_IMPROVEMENT_PERCENTAGE: f64 = 0.0001;  // 0.01%

const MAX_ITERATIONS: usize = 10_000;


pub trait ResidualBlockFunction {
    fn len(&self) -> usize;

    fn calculate(&self, params: &[f64], out: &mut [f64], gradient: &mut [f64]);
}


pub trait ParameterBlockOperator {
    fn update(&self, step: &[f64], params: &mut [f64]);
}


#[derive(Default)]
pub struct LinearParameterBlock {}

impl ParameterBlockOperator for LinearParameterBlock {
    fn update(&self, step: &[f64], params: &mut [f64]) {
        for i in 0..step.len() {
            params[i] += step[i];
        }
    }
}


#[derive(Default)]
pub struct AxisAngleParameterBlock {}

impl ParameterBlockOperator for AxisAngleParameterBlock {
    fn update(&self, step: &[f64], params: &mut [f64]) {
        let increment_axis_angle = vec3d(step[0], step[1], step[2]);
        let cur_axis_angle = vec3d(params[0], params[1], params[2]);

        let new_axis_angle = to_axis_angle(&(
            from_axis_angle(&increment_axis_angle) * from_axis_angle(&cur_axis_angle)));
        for i in 0..3 {
            params[i] = new_axis_angle[i];
        }
    }
}




#[derive(Clone)]
pub struct NonLinearProblemSolution<'a, 'b> {
    solver: &'a NonLinearSolver<'b>,
    pub params: Vec<f64>,
    pub error_sum: f64    
}

impl<'a, 'b> std::fmt::Debug for NonLinearProblemSolution<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NonLinearProblemSolution")
            .field("params", &self.params)
            .field("error_sum", &self.error_sum)
            .finish()
    }
}

impl<'a, 'b> NonLinearProblemSolution<'a, 'b> {

    pub fn param_block(&self, idx: usize) -> &[f64] {
        let spec = &self.solver.param_blocks[idx];
        &self.params[spec.offset..(spec.offset + spec.len)]
    }

}


/// Levenberg-Marquardt non-linear least squares solver
pub struct NonLinearSolver<'a> {
    /// Initial values of all the parameters.
    initial_params: Vec<f64>,

    param_blocks: Vec<ParameterBlockSpec<'a>>,

    num_residuals: usize,

    residual_blocks: Vec<ResidualBlockSpec<'a>>,

    /// Once the error goes below this threshold, we will just stop.
    min_error: f64,

    gradient_descent: bool,

    logging_enabled: bool,

    max_iters: usize,
}

struct ParameterBlockSpec<'a> {
    offset: usize,
    len: usize,
    op: Box<dyn ParameterBlockOperator + 'a>,
    frozen: bool,
}

struct ResidualBlockSpec<'a> {
    offset: usize,
    len: usize,
    param_blocks: Vec<usize>,
    f: Box<dyn ResidualBlockFunction + 'a>
}

struct ParamsState {
    params: VectorXd,
    error: VectorXd,
    error_sum: f64,
    jacobian: MatrixXd,
    cache: Option<(MatrixXd, VectorXd)>
}

impl<'a> NonLinearSolver<'a> {
    pub fn new() -> Self {
        Self {
            initial_params: vec![],
            param_blocks: vec![],
            num_residuals: 0,
            residual_blocks: vec![],
            min_error: 0.000001,
            gradient_descent: false,
            logging_enabled: false,
            max_iters: MAX_ITERATIONS
        }
    }

    pub fn add_parameter_block<P: ParameterBlockOperator + 'a>(
        &mut self,
        initial_params: &[f64], op: P
    ) -> usize {
        let offset = self.initial_params.len();
        self.initial_params.extend_from_slice(initial_params);

        let i = self.param_blocks.len();
        self.param_blocks.push(ParameterBlockSpec {
            offset,
            len: initial_params.len(),
            op: Box::new(op),
            frozen: false,
        });

        i
    }

    pub fn freeze_parameter_block(&mut self, i: usize) {
        self.param_blocks[i].frozen = true;
    }

    pub fn add_residual_block<R: ResidualBlockFunction + 'a>(
        &mut self, param_blocks: &[usize], op: R
    ) {
        
        let offset = self.num_residuals;
        let len = op.len();

        self.num_residuals += len;

        self.residual_blocks.push(ResidualBlockSpec {
            offset,
            len,
            param_blocks: param_blocks.to_vec(),
            f: Box::new(op)
        });
    }

    pub fn set_min_error(&mut self, value: f64) {
        self.min_error = value;
    }

    pub fn set_use_gradient_descent(&mut self) {
        self.gradient_descent = true;
    }

    pub fn enable_logging(&mut self) {
        self.logging_enabled = true;
    }

    pub fn set_max_iterations(&mut self, n: usize) {
        self.max_iters = n;
    }

    #[inline(never)]
    pub fn solve<'b>(&'b self) -> NonLinearProblemSolution<'b, 'a> {
        // Current value of each parameter in the model that we are estimating.
        let mut params = VectorXd::from_slice_with_shape(self.initial_params.len(), 1, &self.initial_params);

        let mut dampening: f64 = 0.01;

        let mut dampening_is_clamped = false;
        let mut last_made_progress = true; 


        if self.logging_enabled {
            println!("JACOBIAN SIZE: {} x {}", self.num_residuals, self.initial_params.len());
        }

        let mut state = self.evaluate_params(params);

        let mut step = VectorXd::zero_with_shape(self.initial_params.len(), 1);

        // Normally this will terminate by:
        // - First there will likely be many rounds of last_made_progress=true
        // - Once the error hits the minimum, we will try ramping the dampening and
        //   the clamping of the dampening to a max value will signal that we hit the
        //   min and can't find a way to make more progress. 
        let mut iters = 0;
        while iters < self.max_iters && (last_made_progress || !dampening_is_clamped) {
            iters += 1;
            
            if self.logging_enabled {
                println!("- {}: Error: {}", iters, state.error_sum);
            }

            if state.error_sum <= self.min_error {
                break;
            }

            let s = Instant::now();
            self.calculate_step(&mut state, dampening, &mut step);
            let e = Instant::now();
            if self.logging_enabled {
                println!("=> calculate_step: {:?}", e - s);
            }

            let mut next_params = state.params.clone();
            self.apply_step(&step, &mut next_params);

            // TODO: I don't need this to compute gradients if the error ends up being bad.
            let next_state = self.evaluate_params(next_params);

            if (
                next_state.error_sum.is_nan() ||
                next_state.error_sum.is_infinite() ||
                ((next_state.error_sum - state.error_sum) / state.error_sum) > -MIN_IMPROVEMENT_PERCENTAGE
            ) {
                dampening *= 10.0;
                last_made_progress = false;
                dampening_is_clamped = false;

                {
                    let clamped_dampening = dampening.min(10_000_000.0).max(0.0000001);
                    dampening_is_clamped = clamped_dampening != dampening;
                    dampening = clamped_dampening;
                }

                continue;
            }

            dampening /= 2.0;
            last_made_progress = true;

            {
                let clamped_dampening = dampening.min(10_000_000.0).max(0.0000001);
                dampening_is_clamped = clamped_dampening != dampening;
                dampening = clamped_dampening;
            }

            state = next_state;
        }

        NonLinearProblemSolution {
            solver: self,
            params: state.params.as_ref().to_vec(),
            error_sum: state.error_sum
        }    
    }

    // TODO: If there are many residuals, then this is heavily parallelizable.
    fn evaluate_params(&self, params: VectorXd) -> ParamsState {
        let s = Instant::now();

        // Each element is the error for a single point with the current parameters.
        let mut error = VectorXd::zero_with_shape(self.num_residuals, 1);

        let mut error_sum = 0.0;

        // Each row is the gradients of each parameter for a single point.
        let mut jacobian = MatrixXd::zero_with_shape(self.num_residuals, self.initial_params.len());

        // Temporary variables used when computing residual blocks.
        let mut current_params = vec![];
        let mut current_gradients = vec![];

        // Calculating the current values for 'error' and 'jacobian' based on
        // the current 'params'.
        for residual_block in &self.residual_blocks {

            let errors_slice = &mut error.as_mut()[residual_block.offset..(residual_block.offset + residual_block.len)];

            if residual_block.param_blocks.len() == 1 {
                let param_block_idx = residual_block.param_blocks[0];
                let params_spec = &self.param_blocks[param_block_idx];
                let params_slice = &params.as_ref()[
                    params_spec.offset..(params_spec.offset + params_spec.len)
                ];

                let gradients_offset = residual_block.offset * self.initial_params.len();
                let gradients_size = residual_block.len * params_spec.len;

                let gradients_slice = &mut jacobian.as_mut()[gradients_offset..(gradients_offset + gradients_size)];

                residual_block.f.calculate(params_slice, errors_slice, gradients_slice);

            } else {
                // Scatter / gather multiple parameters.

                current_params.clear();
                for param_block_idx in &residual_block.param_blocks {
                    let params_spec = &self.param_blocks[*param_block_idx];
                    current_params.extend_from_slice(&params.as_ref()[
                        params_spec.offset..(params_spec.offset + params_spec.len)
                    ]);
                }

                current_gradients.resize(residual_block.len * current_params.len(), 0.0);

                residual_block.f.calculate(&current_params, errors_slice, &mut current_gradients);

                let mut j = 0;
                for i in 0..residual_block.len {
                    for param_block_idx in &residual_block.param_blocks {
                        let params_spec = &self.param_blocks[*param_block_idx];

                        for param_i in 0..params_spec.len {
                            jacobian[(residual_block.offset + i, params_spec.offset + param_i)] = current_gradients[j];
                            j += 1;
                        }
                    }
                }
            }

            for e in errors_slice.iter().cloned() {
                error_sum += e * e;
            }
        }

        let e = Instant::now();
        if self.logging_enabled {
            println!("=> evaluate_params: {:?}", e - s);
        }

        ParamsState {
            params,
            error,
            error_sum,
            jacobian,
            cache: None,
        }
    }

    // NOTE: The state is just mutable here to support caching intermediate products.
    fn calculate_step(&self, state: &mut ParamsState, dampening: f64, step: &mut VectorXd) {
        let mut num_unfrozen = 0;
        for param_spec in &self.param_blocks {
            if param_spec.frozen {
                continue;
            }

            num_unfrozen += param_spec.len;
        }
        
        // Extract all non-frozen parameter columns from the jacobian matrix.
        let mut jacobian_thin = MatrixXd::zero_with_shape(self.num_residuals, num_unfrozen);
        {
            let mut input_i = 0;
            let mut output_i = 0;
            for param_spec in &self.param_blocks {
                if param_spec.frozen {
                    input_i += param_spec.len;
                    continue;
                }

                for _ in 0..param_spec.len {
                    jacobian_thin.col_mut(output_i).copy_from(&state.jacobian.col(input_i));
                    input_i += 1;
                    output_i += 1;
                }
            }
        }

        let step_thin = {
            if self.gradient_descent || dampening >= 1000.0 {
                // At high dampening, the step can be approximated as gradient descent:
                // 'step = (1/dampening) J^transpose * error'

                let mut dampening_mat = square_diag(&jacobian_thin);

                // Apply dampening coefficient and 'invert' the matrix.
                for i in 0..dampening_mat.len() {
                    dampening_mat[i] = 1.0 / (dampening_mat[i] * dampening);
                }

                dampening_mat.cwise_mul_assign((jacobian_thin.as_transpose() * &state.error));

                dampening_mat

            } else if jacobian_thin.len() < 100 {
                // For small matrices (threshold not well tuned), direct inversion is doing to be faster.

                // TODO: Don't need to clone 'b'
                let (mut a, b) = {
                    if let Some(v) = &state.cache {
                        v.clone()
                    } else {
                        let a = jacobian_thin.as_transpose() * &jacobian_thin;
                        let b = jacobian_thin.as_transpose() * &state.error;

                        state.cache = Some((a.clone(), b.clone()));

                        (a, b)
                    }
                };

                // Basically same as the following but faster:
                // let dampening_mat = diag(&jacobian_square) * dampening;
                // a += dampening_mat;
                let d = 1.0 + dampening;
                for i in 0..a.rows() {
                    a[(i, i)] *= d;
                }

                let a_inv = match a.inverse() {
                    Some(v) => v,
                    None => return
                };

                a_inv * b
            } else {

                let (mut a, b) = {
                    if let Some(v) = &state.cache {
                        v.clone()
                    } else {
                        let mut a = MatrixXd::zero_with_shape(jacobian_thin.cols(), jacobian_thin.cols());
                        let mut b = VectorXd::zero_with_shape(jacobian_thin.cols(), 1);

                        vision_ffi::compute_jtj(
                            jacobian_thin.as_ref(),
                            state.error.as_ref(),
                            jacobian_thin.rows(),
                            jacobian_thin.cols(),
                            a.as_mut(),
                            b.as_mut()
                        );

                        state.cache = Some((a.clone(), b.clone()));

                        (a, b)
                    }
                };


                let d = 1.0 + dampening;
                for i in 0..a.rows() {
                    a[(i, i)] *= d;
                }

                let mut step_thin = VectorXd::zero_with_shape(jacobian_thin.cols(), 1);

                let s = Instant::now();

                if !vision_ffi::solve_sparse_ldlt(a.as_ref(), b.as_ref(), step_thin.as_mut()) {
                    //
                }

                let e = Instant::now();

                if self.logging_enabled {
                    println!("=> Solve Time: {:?}", e - s);
                }


                step_thin
            }
        };

        // Expand step to include zeros for all frozen parameters.
        {
            let mut input_i = 0;
            let mut output_i = 0;
            for param_spec in &self.param_blocks {
                if param_spec.frozen {
                    output_i += param_spec.len;
                    continue;
                }

                for _ in 0..param_spec.len {
                    step[output_i] = step_thin[input_i];
                    input_i += 1;
                    output_i += 1;
                }
            }
        }
    }

    fn apply_step(&self, step: &VectorXd, params: &mut VectorXd) {
        for param_spec in &self.param_blocks {
            if param_spec.frozen {
                continue;
            }

            param_spec.op.update(
                &step.as_ref()[param_spec.offset..(param_spec.offset + param_spec.len)],
                &mut params.as_mut()[param_spec.offset..(param_spec.offset + param_spec.len)],
            );
        }
    }
}

/// Efficiently calculates the diagonal entries of '(mat^T) mat'
fn square_diag(mat: &MatrixXd) -> VectorXd {
    let mut out = VectorXd::zero_with_shape(mat.cols(), 1);

    for i in 0..mat.cols() {
        for j in 0..mat.rows() {
            let v = mat[(j, i)];
            out[i] += v * v;
        }
    }

    out
}

fn diag(mat: &MatrixXd) -> MatrixXd {
    let mut out = MatrixXd::zero_with_shape(mat.rows(), mat.cols());
    for i in 0..mat.rows() {
        out[(i, i)] = mat[(i, i)]
    }
    out
}

#[cfg(test)]
mod tests {

    use super::*;

    // Model is 'y = Ax^2 + B'
    struct QuadraticResidual {
        // These are hidden from the solver and we expect the solver to be able to find them
        // just based on the data.
        a: f64,
        b: f64,

        x: f64,
    }

    /*
    error = (expected - Actual)^2

    errr = (expected - x * A + B)^2
    derror = 2 * (error) * -X
    */

    impl ResidualBlockFunction for QuadraticResidual {
        fn len(&self) -> usize {
            1
        }

        fn calculate(&self, params: &[f64], out: &mut [f64], gradient: &mut [f64]) {
            let expected_y = self.a * self.x + self.b;

            let y = params[0] * self.x + params[1];
            let error = expected_y - y;
            out[0] = error;

            gradient[0] = self.x;  // * 2.0 * error;
            gradient[1] = 1.0;
        }
    }

    #[test]
    fn solve_nonlinear_works() {
        let mut solver = NonLinearSolver::new();
        // solver.set_use_gradient_descent();

        let params_idx = solver.add_parameter_block(&[0.0, 0.0], LinearParameterBlock::default());

        for x in [-2.0, -1.0, 0.0, 1.0, 2.0] {
            solver.add_residual_block(&[params_idx], QuadraticResidual {
                a: 1.0,
                b: 2.0,
                x
            });
        }

        let solution = solver.solve();
        assert_eq!(solution.params, &[1.0, 2.0]);
    }

}
