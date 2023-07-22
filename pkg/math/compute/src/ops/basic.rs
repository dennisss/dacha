use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*};

use super::{cwise_product, matmul};

#[derive(Debug)]
pub struct IdentityOp {
    pub input_spec: TensorSpec,
}

#[async_trait]
impl Operation for IdentityOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Identity",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![self.input_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let v = context.get_input(0)?;
        context.set_output(0, v);
        Ok(())
    }

    // This is the exact same as the Add implementation
    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        Ok(context.doutputs.to_vec())
    }
}

// input_type

#[derive(Debug)]
pub struct CastOp {
    pub input_spec: TensorSpec,
    pub dtype: DataType,
}

#[async_trait]
impl Operation for CastOp {
    fn signature(&self) -> OperationSignature {
        let mut output_spec = self.input_spec.clone();
        output_spec.dtype = self.dtype;

        OperationSignature {
            name: "Cast",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![output_spec],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let v = context.get_input(0)?;
        context.set_output(0, v.cast(self.dtype));
        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        Ok(vec![context.inputs[0].cast(self.input_spec.dtype)])
    }
}
