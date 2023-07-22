use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*, tensor_array_do};

use super::{cwise_product, matmul};

#[derive(Debug)]
pub struct BroadcastToOp {
    pub input_spec: TensorSpec,
}

#[async_trait]
impl Operation for BroadcastToOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "BroadcastTo",
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

        let out = tensor_array_do!(
            &*input_tensor,
            v,
            Tensor::from(v.broadcast_to(&target_shape))
        );

        context.set_output(0, out);

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        Ok(vec![
            context.doutputs[0].reduce_to(context.inputs[0].shape())
        ])
    }
}

/// Reduces the input tensor to be given shape.
///
/// Axes are summed up until we reach the target shape. The input shape and
/// target shape must be broadcast compatible.
///
/// Inputs:
/// [0] Tensor to be reduced.
/// [1] Target shape (uint32)
#[derive(Debug)]
pub struct ReduceToShapeOp {
    pub input_spec: TensorSpec,
}

#[async_trait]
impl Operation for ReduceToShapeOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "ReduceToShape",
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
        // TODO: Don't need to do anything if the input is already the right shape (or
        // reshapeable to it)

        let input_tensor = context.get_input(0)?;

        let input_shape = input_tensor.shape();

        let target_shape = context.get_shape_input(1)?;

        let mut normalized_target_shape = target_shape.clone();

        while normalized_target_shape.len() > input_shape.len() {
            assert_eq!(normalized_target_shape[0], 1);
            normalized_target_shape.remove(0);
        }

        while normalized_target_shape.len() < input_shape.len() {
            normalized_target_shape.insert(0, 1);
        }

        let mut reduction_axes = vec![];

        for i in 0..input_shape.len() {
            let in_i = input_shape[i];
            let target_i = normalized_target_shape[i];

            if in_i != target_i {
                assert_eq!(target_i, 1);
                reduction_axes.push(i as isize);
            }
        }

        // TODO: Make sure this does nothing if
        let output = input_tensor.array::<f32>().unwrap().sum(&reduction_axes);

        // May need to trim leading 1 dimensions.
        let output = output.reshape(&target_shape);

        context.set_output(0, output);

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        todo!()
    }
}
