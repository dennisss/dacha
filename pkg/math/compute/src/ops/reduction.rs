use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*};

use super::{cwise_product, matmul};

#[derive(Debug)]
pub struct SumOp {
    pub input_spec: TensorSpec,
    pub axes: Vec<isize>,
}

#[async_trait]
impl Operation for SumOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Sum",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![self.input_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;

        let x_value: &Array<f32> = x.array::<f32>().unwrap();

        let sum = x_value.sum(&self.axes);
        context.set_output(0, Tensor::from(sum));

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        // Broadcast back up to the shape of the input.

        let dinput = context.doutputs[0].broadcast_to(context.inputs[0].shape());

        Ok(vec![dinput])
    }
}
