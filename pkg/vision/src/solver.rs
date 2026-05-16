use math::matrix::{VectorXf, MatrixXf, Vector3f, Matrix3f, Vector2f, vec2f, vec3f};
use math::matrix::axis_angle::*;

/// 
const MIN_IMPROVEMENT_PERCENTAGE: f32 = 0.001;  // 0.1%

const MAX_ITERATIONS: usize = 10_000;


pub trait ResidualBlockFunction {
    fn len(&self) -> usize;

    fn calculate(&self, params: &[f32], out: &mut [f32], gradient: &mut [f32]);
}


pub trait ParameterBlockOperator {
    fn update(&self, step: &[f32], params: &mut [f32]);
}


#[derive(Default)]
pub struct LinearParameterBlock {}

impl ParameterBlockOperator for LinearParameterBlock {
    fn update(&self, step: &[f32], params: &mut [f32]) {
        for i in 0..step.len() {
            params[i] += step[i];
        }
    }
}


#[derive(Default)]
pub struct AxisAngleParameterBlock {}

impl ParameterBlockOperator for AxisAngleParameterBlock {
    fn update(&self, step: &[f32], params: &mut [f32]) {
        let increment_axis_angle = vec3f(step[0], step[1], step[2]);
        let cur_axis_angle = vec3f(params[0], params[1], params[2]);

        let new_axis_angle = to_axis_angle(&(
            from_axis_angle(&increment_axis_angle) * from_axis_angle(&cur_axis_angle)));
        for i in 0..3 {
            params[i] = new_axis_angle[i];
        }
    }
}




#[derive(Clone, Debug)]
pub struct NonLinearProblemSolution {
    pub params: Vec<f32>,
    pub error_sum: f32    
}

/// Levenberg-Marquardt non-linear least squares solver
pub struct NonLinearSolver<'a> {
    /// Initial values of all the parameters.
    initial_params: Vec<f32>,

    param_blocks: Vec<ParameterBlockSpec<'a>>,

    num_residuals: usize,

    residual_blocks: Vec<ResidualBlockSpec<'a>>,

    /// Once the error goes below this threshold, we will just stop.
    min_error: f32,

    gradient_descent: bool,
}

struct ParameterBlockSpec<'a> {
    offset: usize,
    len: usize,
    op: Box<dyn ParameterBlockOperator + 'a>,
}

struct ResidualBlockSpec<'a> {
    offset: usize,
    len: usize,
    param_blocks: Vec<usize>,
    f: Box<dyn ResidualBlockFunction + 'a>
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
        }
    }

    pub fn add_parameter_block<P: ParameterBlockOperator + 'a>(
        &mut self,
        initial_params: &[f32], op: P
    ) -> usize {
        let offset = self.initial_params.len();
        self.initial_params.extend_from_slice(initial_params);

        let i = self.param_blocks.len();
        self.param_blocks.push(ParameterBlockSpec {
            offset,
            len: initial_params.len(),
            op: Box::new(op)
        });

        i
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

    pub fn set_min_error(&mut self, value: f32) {
        self.min_error = value;
    }

    pub fn set_use_gradient_descent(&mut self) {
        self.gradient_descent = true;
    }

    pub fn solve(&self) -> NonLinearProblemSolution {
        // Current value of each parameter in the model that we are estimating.
        let mut params = VectorXf::from_slice_with_shape(self.initial_params.len(), 1, &self.initial_params);
        
        // Last parameter set and it's overall error sum.
        let mut last_params: Option<(VectorXf, f32)> = None;

        // Each element is the error for a single point with the current parameters.
        let mut error = VectorXf::zero_with_shape(self.num_residuals, 1);

        let mut error_sum = 0.0;

        // Each row is the gradients of each parameter for a single point.
        let mut jacobian = MatrixXf::zero_with_shape(self.num_residuals, self.initial_params.len());

        let mut dampening: f32 = 0.01;

        let mut dampening_is_clamped = false;
        let mut last_made_progress = true; 

        // Temporary variables used when computing residual blocks.
        let mut current_params = vec![];
        let mut current_errors = vec![];
        let mut current_gradients = vec![];

        // Normally this will terminate by:
        // - First there will likely be many rounds of last_made_progress=true
        // - Once the error hits the minimum, we will try ramping the dampening and
        //   the clamping of the dampening to a max value will signal that we hit the
        //   min and can't find a way to make more progress. 
        let mut iters = 0;
        while iters < MAX_ITERATIONS && (last_made_progress || !dampening_is_clamped) {
            iters += 1;
            
            error_sum = 0.0;

            // Calculating the current values for 'error' and 'jacobian' based on
            // the current 'params'.
            for residual_block in &self.residual_blocks {
                current_params.clear();
                for param_block_idx in &residual_block.param_blocks {
                    let params_spec = &self.param_blocks[*param_block_idx];
                    current_params.extend_from_slice(&params.as_ref()[
                        params_spec.offset..(params_spec.offset + params_spec.len)
                    ]);
                }

                current_errors.resize(residual_block.len, 0.0);
                current_gradients.resize(residual_block.len * current_params.len(), 0.0);

                residual_block.f.calculate(&current_params, &mut current_errors, &mut current_gradients);

                let mut j = 0;
                for i in 0..current_errors.len() {
                    error_sum += current_errors[i];
                    error[residual_block.offset + i] = current_errors[i];

                    for param_block_idx in &residual_block.param_blocks {
                        let params_spec = &self.param_blocks[*param_block_idx];

                        for param_i in 0..params_spec.len {
                            jacobian[(residual_block.offset + i, params_spec.offset + param_i)] = current_gradients[j];
                            j += 1;
                        }
                    }
                }
            }

            if params.len() > 20 {
                println!("- Error: {}", error_sum);

                println!("{:?}", params);
            }

            if error_sum <= self.min_error {
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


            let step = {
                if self.gradient_descent {
                    let mut step = VectorXf::zero_with_shape(self.initial_params.len(), 1);

                    for i in 0..jacobian.rows() {
                        for j in 0..jacobian.cols() {
                            step[j] += jacobian[(i, j)];
                        }
                    }

                    step * dampening

                    // step * dampening * (1.0 / (jacobian.rows() as f32))

                } else {
                    let jacobian_square = jacobian.transpose() * &jacobian;

                    let dampening_mat = diag(&jacobian_square) * dampening;

                    let a = jacobian_square + dampening_mat;

                    let a_inv = a.inverse();

                    a_inv * jacobian.transpose() * &error
                }
            };

            for param_spec in &self.param_blocks {
                param_spec.op.update(
                    &step.as_ref()[param_spec.offset..(param_spec.offset + param_spec.len)],
                    &mut params.as_mut()[param_spec.offset..(param_spec.offset + param_spec.len)],
                );
            }

            // if self.gradient_descent {
            //     println!("NexT: {:?}", params);
            // }
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
    struct QuadraticResidual {
        // These are hidden from the solver and we expect the solver to be able to find them
        // just based on the data.
        a: f32,
        b: f32,

        x: f32,
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

        fn calculate(&self, params: &[f32], out: &mut [f32], gradient: &mut [f32]) {
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
