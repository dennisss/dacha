use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*, tensor_array_do};

#[derive(Debug)]
pub struct ConstantOp {
    pub value: Tensor,
}

#[async_trait]
impl Operation for ConstantOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Const",
            inputs: vec![],
            outputs: vec![TensorSpec {
                dtype: self.value.dtype(),
            }],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        context.set_output(0, self.value.clone());
        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        Ok(vec![])
    }
}

#[derive(Debug)]
pub struct FillOp {
    pub input_spec: TensorSpec,
}

#[async_trait]
impl Operation for FillOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Fill",
            inputs: vec![
                self.input_spec.clone(),
                TensorSpec {
                    dtype: DataType::Uint32,
                },
            ],
            outputs: vec![self.input_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let input_tensor = context.get_input(0)?;
        let target_shape = context.get_shape_input(1)?;

        assert!(input_tensor.shape().len() == 0);

        let out = tensor_array_do!(
            &*input_tensor,
            v,
            Tensor::from(Array::fill(&target_shape, v[0]))
        );
        context.set_output(0, out);
        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        todo!()
    }
}

// Zeros

// Ones
