use alloc::boxed::Box;
use alloc::vec::Vec;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*};

use super::{cwise_product, matmul};

#[derive(Debug)]
pub struct AddOp {
    pub input_specs: Vec<TensorSpec>,
}

#[async_trait]
impl Operation for AddOp {
    fn signature(&self) -> OperationSignature {
        for s in &self.input_specs[1..] {
            assert_eq!(s.dtype, self.input_specs[0].dtype);
        }

        OperationSignature {
            name: "Add",
            inputs: self.input_specs.clone(),
            outputs: vec![self.input_specs[0].clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;
        let y = context.get_input(1)?;

        let x_value = x.array::<f32>().unwrap();
        let y_value = y.array::<f32>().unwrap();

        // TODO: Must verify shapes are compatible or implement broadcasting (returning
        // proper errors on incompatibility).
        let z = x_value.try_cwise_add(y_value).unwrap();

        context.set_output(0, Tensor::from(z));

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        let mut input_grads = vec![];

        for i in 0..context.inputs.len() {
            // Implicitly multiplying doutput by 1.
            input_grads.push(context.doutputs[0].reduce_to(context.inputs[i].shape()));
        }

        Ok(input_grads)
    }
}

#[derive(Debug)]
pub struct MulOp {
    pub input_specs: Vec<TensorSpec>,
}

#[async_trait]
impl Operation for MulOp {
    fn signature(&self) -> OperationSignature {
        for s in &self.input_specs[1..] {
            assert_eq!(s.dtype, self.input_specs[0].dtype);
        }

        OperationSignature {
            name: "Mul",
            inputs: self.input_specs.clone(),
            outputs: vec![self.input_specs[0].clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;
        let y = context.get_input(1)?;

        let x_value = x.array::<f32>().unwrap();
        let y_value = y.array::<f32>().unwrap();

        // TODO: Must verify shapes are compatible or implement broadcasting.

        let z = x_value.cwise_mul(y_value);

        context.set_output(0, Tensor::from(z));

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        // perform cwise_mul(doutputs[0], inputs[i])
        // (guaranteed to have )

        let mut input_grads = vec![];

        for i in 0..context.inputs.len() {
            let mut mul_parts = vec![context.doutputs[0].clone()];

            for j in 0..context.inputs.len() {
                if i != j {
                    mul_parts.push(context.inputs[j].clone());
                }
            }

            input_grads.push(cwise_product(&mul_parts).reduce_to(context.inputs[i].shape()));
        }

        Ok(input_grads)
    }
}

#[derive(Debug)]
pub struct DivOp {
    pub input_specs: Vec<TensorSpec>,
}

#[async_trait]
impl Operation for DivOp {
    fn signature(&self) -> OperationSignature {
        for s in &self.input_specs[1..] {
            assert_eq!(s.dtype, self.input_specs[0].dtype);
        }

        OperationSignature {
            name: "Div",
            inputs: self.input_specs.clone(),
            outputs: vec![self.input_specs[0].clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;
        let y = context.get_input(1)?;

        let x_value = x.array::<f32>().unwrap();
        let y_value = y.array::<f32>().unwrap();

        // TODO: Must verify shapes are compatible or implement broadcasting.

        let z = x_value.cwise_div(y_value);

        context.set_output(0, Tensor::from(z));

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        let x = &context.inputs[0];
        let y = &context.inputs[1];

        // dx = 1 / y
        let dx = 1 / y;
        let dx = dx.reduce_to(x.shape());

        // dy = -x / y^2
        let dy = (-1 * x) / (y * y);
        let dy = dy.reduce_to(y.shape());

        Ok(vec![dx, dy])
    }
}
