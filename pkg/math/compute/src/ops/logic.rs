/*
MaxOp
    cond(x > y, x, y)
    - cwise_binary_op

CompareOp


SelectOp
    - Pick one or the other

*/

use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*, tensor_array_do};

#[derive(Debug)]
pub struct MaxOp {
    pub tensor_specs: Vec<TensorSpec>,
}

#[async_trait]
impl Operation for MaxOp {
    fn signature(&self) -> OperationSignature {
        let dtype = self.tensor_specs[0].dtype;
        for spec in &self.tensor_specs[1..] {
            assert_eq!(dtype, spec.dtype);
        }

        OperationSignature {
            name: "Max",
            inputs: self.tensor_specs.clone(),
            outputs: vec![self.tensor_specs[0].clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;
        let y = context.get_input(1)?;

        let x_arr = x.array::<f32>().unwrap();
        let y_arr = x.array::<f32>().unwrap();

        let z_arr = x_arr.cwise_max(y_arr).unwrap();

        context.set_output(0, z_arr);

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        todo!()
    }
}
