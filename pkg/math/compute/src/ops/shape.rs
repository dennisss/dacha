use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*};
use crate::{fill, tensor_array_do};

/// Retrieves the shape of an input tensor.
/// Returns a shape [N] uint32 tensor.
#[derive(Debug)]
pub struct ShapeOp {
    pub input_spec: TensorSpec,
}

#[async_trait]
impl Operation for ShapeOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Shape",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![TensorSpec {
                dtype: DataType::Uint32,
            }],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let v: Tensor = context.get_input(0)?;
        let shape = v.shape().iter().map(|v| *v as u32).collect::<Vec<_>>();
        let shape_t = Tensor::from(Array::<u32>::from(shape));

        context.set_output(0, shape_t);
        Ok(())
    }

    // TODO: When observing nodes like this during gradient traversal, we should
    // just prune them as zeros don't contribute to the gradient.
    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        // TODO: Replace with zeros_like

        let out: Tensor = match context.inputs[0].spec().dtype {
            DataType::Float32 => Array::<f32>::zeros(&[]).into(),
            DataType::Uint8 => Array::<u8>::zeros(&[]).into(),
            DataType::Uint32 => Array::<u32>::zeros(&[]).into(),
        };

        Ok(vec![constant(out)])
    }
}

#[derive(Debug)]
pub struct SizeOp {
    pub input_spec: TensorSpec,
}

#[async_trait]
impl Operation for SizeOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Size",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![TensorSpec {
                dtype: DataType::Uint32,
            }],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let v = context.get_input(0)?;
        context.set_output(0, v.size() as u32);
        Ok(())
    }

    // TODO: Dedup with the shape grasdient.
    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        let zero: Tensor = match context.inputs[0].spec().dtype {
            DataType::Float32 => Array::<f32>::zeros(&[]).into(),
            DataType::Uint8 => Array::<u8>::zeros(&[]).into(),
            DataType::Uint32 => Array::<u32>::zeros(&[]).into(),
        };

        let out = fill(constant(zero), context.inputs[0].shape());

        Ok(vec![out])
    }
}

#[derive(Debug)]
pub struct ExpandDimsOp {
    pub input_spec: TensorSpec,
    pub axes: Vec<isize>,
}

#[async_trait]
impl Operation for ExpandDimsOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "ExpandDims",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![self.input_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let v = context.get_input(0)?;

        let arr: &TensorArray = &*v;
        let z = tensor_array_do!(arr, v, Tensor::from(v.expand_dims(&self.axes)));

        // TODO: Optimize reshape to always use the same internal data buffer.

        context.set_output(0, z);
        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        Ok(vec![context.doutputs[0].squeeze(&self.axes)])
    }
}

#[derive(Debug)]
pub struct SqueezeOp {
    pub input_spec: TensorSpec,
    pub axes: Vec<isize>,
}

#[async_trait]
impl Operation for SqueezeOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Squeeze",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![self.input_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let v = context.get_input(0)?;

        let arr: &TensorArray = &*v;
        let z = tensor_array_do!(arr, v, Tensor::from(v.squeeze(&self.axes)));

        context.set_output(0, z);
        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        Ok(vec![context.doutputs[0].expand_dims(&self.axes)])
    }
}

#[derive(Debug)]
pub struct SwapAxesOp {
    pub input_spec: TensorSpec,
    pub first_axis: isize,
    pub second_axis: isize,
}

#[async_trait]
impl Operation for SwapAxesOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "SwapAxes",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![self.input_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;

        let z = tensor_array_do!(
            &*x,
            v,
            Tensor::from(v.swap_axes(self.first_axis, self.second_axis))
        );

        context.set_output(0, Tensor::from(z));

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        Ok(vec![
            context.doutputs[0].swap_axes(self.first_axis, self.second_axis)
        ])
    }
}
