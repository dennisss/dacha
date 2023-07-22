use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*};

use super::{cwise_product, matmul};

#[derive(Debug)]
pub struct MatMulOp {
    pub input_specs: Vec<TensorSpec>,
}

#[async_trait]
impl Operation for MatMulOp {
    fn signature(&self) -> OperationSignature {
        for s in &self.input_specs[1..] {
            assert_eq!(s.dtype, self.input_specs[0].dtype);
        }

        OperationSignature {
            name: "MatMul",
            inputs: self.input_specs.clone(),
            outputs: vec![self.input_specs[0].clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;
        let y = context.get_input(1)?;

        let x_value = x.array::<f32>().unwrap();
        let y_value = y.array::<f32>().unwrap();

        let z = x_value.matmul(&y_value);

        context.set_output(0, Tensor::from(z));

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        // shape (n, m)
        let x = &context.inputs[0];

        // shape (m, h)
        let y = &context.inputs[1];

        // shape (n, h)
        let dz = &context.doutputs[0];

        // (n, m) = (n, h) o (h, )
        let dx = matmul(dz.clone(), y.swap_axes(-1, -2)).reduce_to(x.shape());

        let dy = matmul(x.swap_axes(-1, -2), dz.clone()).reduce_to(y.shape());

        Ok(vec![dx, dy])
    }
}
