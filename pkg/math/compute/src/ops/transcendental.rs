use alloc::boxed::Box;
use alloc::vec::Vec;
use core::f32::consts::E;

use common::errors::*;
use math::array::Array;

use crate::{constant, graph::*};

use super::{cwise_product, matmul};

#[derive(Debug)]
pub struct ExpOp {
    pub input_spec: TensorSpec,
}

#[async_trait]
impl Operation for ExpOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Exp",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![self.input_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;

        let x_value = x.array::<f32>().unwrap();
        let out = x_value.map(|v| v.exp());

        context.set_output(0, Tensor::from(out));

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        let ex = &context.outputs[0];
        Ok(vec![&context.doutputs[0] * ex])
    }
}

#[derive(Debug)]
pub struct LogOp {
    pub input_spec: TensorSpec,
    pub base: f32,
}

#[async_trait]
impl Operation for LogOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Log",
            inputs: vec![self.input_spec.clone()],
            outputs: vec![self.input_spec.clone()],
        }
    }

    async fn execute(&self, context: &mut OperationExecuteContext) -> Result<()> {
        let x = context.get_input(0)?;

        let x_value = x.array::<f32>().unwrap();

        let out = {
            if self.base == E {
                x_value.map(|v| v.ln())
            } else if self.base == 2.0 {
                x_value.map(|v| v.log2())
            } else if self.base == 10.0 {
                x_value.map(|v| v.log10())
            } else {
                x_value.map(|v| v.log(self.base))
            }
        };

        context.set_output(0, Tensor::from(out));

        Ok(())
    }

    fn gradient(&self, context: OperationGradientContext) -> Result<Vec<Output>> {
        // d(log_a(x))/dx = 1 / (x ln(a))
        let x = &context.inputs[0];

        let t = self.base.ln();

        let dx = &context.doutputs[0] / (x * t);

        Ok(vec![dx])
    }
}
