use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::graph::*;

#[derive(Clone, Debug)]
pub struct InputSpec {
    pub value_spec: TensorSpec,

    /// If true, this input is expected to be mutated during a training process
    /// to optimize the function outputs.
    pub trainable: bool,

    /// Value to use for this input if none is provided in the inputs.
    pub initial_value: Option<Tensor>,
}

/// Placeholder operations that is used to represent edges that will be fed
/// dynamic values at graph execution time. This op itself should never get
/// executed.
#[derive(Debug)]
pub struct InputOp {
    pub spec: InputSpec,
}

#[async_trait]
impl Operation for InputOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Input",
            inputs: vec![],
            outputs: vec![self.spec.value_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        Err(err_msg("Input was not fed any precomputed value."))
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        Ok(vec![])
    }
}
