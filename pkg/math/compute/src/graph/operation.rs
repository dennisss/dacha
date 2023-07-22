use alloc::boxed::Box;
use alloc::vec::Vec;
use common::any::AsAny;
use core::fmt::Debug;

use common::errors::*;

use crate::graph::executor::OperationExecuteContext;
use crate::graph::graph::{Graph, NodeId, Output};
use crate::graph::tensor::DataType;
use crate::graph::tensor::Tensor;

#[derive(Clone, Debug)]
pub struct TensorSpec {
    pub dtype: DataType,
}

#[derive(Clone, Debug)]
pub struct OperationSignature {
    pub name: &'static str,
    pub inputs: Vec<TensorSpec>,
    pub outputs: Vec<TensorSpec>,
}

pub struct OperationGradientContext<'a> {
    pub inputs: &'a [Output],
    pub outputs: &'a [Output],
    pub doutputs: &'a [Output],
}

#[async_trait]
pub trait Operation: 'static + Send + Sync + Debug + AsAny {
    // TODO: Ideally would support serialization.

    fn signature(&self) -> OperationSignature;

    /// Evaluates the operation on fully materialized input values.
    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()>;

    /// Computes the partial derivatives dy/dinput for each input of this op.
    /// 'doutputs' is a list of derivatives relative to each output
    /// (dy/doutput).
    ///
    /// A gradient MUST be the same shape as each input parameter.
    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>>;
}
