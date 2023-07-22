mod arithmetic;
mod basic;
mod broadcast;
mod constant;
mod input;
mod logic;
mod matrix;
mod reduction;
mod shape;
mod transcendental;

use crate::graph::*;
pub use arithmetic::*;
pub use basic::*;
pub use broadcast::*;
pub use constant::*;
pub use input::*;
pub use logic::*;
pub use matrix::*;
pub use reduction::*;
pub use shape::*;
pub use transcendental::*;

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

// Helpers for graph building.

pub fn input(dtype: DataType) -> Output {
    let graph = GraphContext::current_graph().unwrap();
    graph
        .add_node(
            None,
            InputOp {
                spec: InputSpec {
                    value_spec: TensorSpec { dtype },
                    trainable: false,
                    initial_value: None,
                },
            },
            &[],
        )
        .remove(0)
}

pub fn variable<T: Into<Tensor>>(name: &str, initial_value: T) -> Output {
    let graph = GraphContext::current_graph().unwrap();
    let initial_value = initial_value.into();
    graph
        .add_node(
            Some(name),
            InputOp {
                spec: InputSpec {
                    value_spec: TensorSpec {
                        dtype: initial_value.dtype(),
                    },
                    trainable: true,
                    initial_value: Some(initial_value),
                },
            },
            &[],
        )
        .remove(0)
}

pub fn identity(x: Output) -> Output {
    let graph = x.graph().clone();
    graph
        .add_node(
            None,
            IdentityOp {
                input_spec: x.spec(),
            },
            &[x],
        )
        .remove(0)
}

impl Output {
    pub fn cast(&self, dtype: DataType) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                CastOp {
                    input_spec: self.spec(),
                    dtype,
                },
                &[self.clone()],
            )
            .remove(0)
    }

    pub fn shape(&self) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                ShapeOp {
                    input_spec: self.spec(),
                },
                &[self.clone()],
            )
            .remove(0)
    }

    pub fn size(&self) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                SizeOp {
                    input_spec: self.spec(),
                },
                &[self.clone()],
            )
            .remove(0)
    }

    pub fn swap_axes(&self, i: isize, j: isize) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                SwapAxesOp {
                    input_spec: self.spec(),
                    first_axis: i,
                    second_axis: j,
                },
                &[self.clone()],
            )
            .remove(0)
    }

    pub fn sum(&self, axes: &[isize], keep_dims: bool) -> Output {
        let graph = self.graph().clone();
        let out = graph
            .add_node(
                None,
                SumOp {
                    input_spec: self.spec(),
                    axes: axes.to_vec(),
                },
                &[self.clone()],
            )
            .remove(0);

        if !keep_dims {
            return out.squeeze(axes);
        }

        out
    }

    pub fn exp(&self) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                ExpOp {
                    input_spec: self.spec(),
                },
                &[self.clone()],
            )
            .remove(0)
    }

    pub fn broadcast_to(&self, shape: Output) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                BroadcastToOp {
                    input_spec: self.spec(),
                },
                &[self.clone(), shape],
            )
            .remove(0)
    }

    pub fn reduce_to(&self, shape: Output) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                ReduceToShapeOp {
                    input_spec: self.spec(),
                },
                &[self.clone(), shape],
            )
            .remove(0)
    }

    pub fn expand_dims(&self, axes: &[isize]) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                ExpandDimsOp {
                    input_spec: self.spec(),
                    axes: axes.into(),
                },
                &[self.clone()],
            )
            .remove(0)
    }

    pub fn squeeze(&self, axes: &[isize]) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                SqueezeOp {
                    input_spec: self.spec(),
                    axes: axes.into(),
                },
                &[self.clone()],
            )
            .remove(0)
    }

    pub fn ln(&self) -> Output {
        let graph = self.graph().clone();
        graph
            .add_node(
                None,
                LogOp {
                    input_spec: self.spec(),
                    base: core::f32::consts::E,
                },
                &[self.clone()],
            )
            .remove(0)
    }
}

pub trait IntoOutput {
    fn into_output(self, dtype_hint: DataType) -> Output;
}

impl IntoOutput for Output {
    fn into_output(self, dtype_hint: DataType) -> Output {
        self
    }
}

impl IntoOutput for &Output {
    fn into_output(self, dtype_hint: DataType) -> Output {
        self.clone()
    }
}

impl IntoOutput for i32 {
    fn into_output(self, dtype_hint: DataType) -> Output {
        // TODO: Support more target dtypes.
        assert_eq!(dtype_hint, DataType::Float32);
        constant(self as f32)
    }
}

impl IntoOutput for f32 {
    fn into_output(self, dtype_hint: DataType) -> Output {
        constant(self)
    }
}

pub fn constant<T: Into<Tensor>>(value: T) -> Output {
    let graph = GraphContext::current_graph().unwrap();
    graph
        .add_node(
            None,
            ConstantOp {
                value: value.into(),
            },
            &[],
        )
        .remove(0)
}

pub fn fill(value: Output, shape: Output) -> Output {
    let graph = GraphContext::current_graph().unwrap();
    graph
        .add_node(
            None,
            FillOp {
                input_spec: value.spec(),
            },
            &[value, shape],
        )
        .remove(0)
}

macro_rules! cwise_binary_op {
    ($OpAssign:ident, $op_assign:ident, $Op:ident, $op:ident, $f:expr) => {
        impl<O: IntoOutput> $Op<O> for Output {
            type Output = Output;

            fn $op(self, rhs: O) -> Self::Output {
                let dtype = self.dtype();
                $f(self, rhs.into_output(dtype))
            }
        }

        impl<O: IntoOutput> $Op<O> for &Output {
            type Output = Output;

            fn $op(self, rhs: O) -> Self::Output {
                self.clone().$op(rhs.into_output(self.dtype()))
            }
        }

        // For operations involving a Rust primitive on the left hand side.
        // e.g. '1 + x'
        impl $Op<Output> for i32 {
            type Output = Output;

            fn $op(self, rhs: Output) -> Self::Output {
                let lhs = self.into_output(rhs.dtype());
                lhs.$op(rhs)
            }
        }

        impl $Op<&Output> for i32 {
            type Output = Output;

            fn $op(self, rhs: &Output) -> Self::Output {
                let lhs = self.into_output(rhs.dtype());
                lhs.$op(rhs)
            }
        }

        // TODO: Also implement primitive ops for f32

        impl $OpAssign<Output> for Output {
            fn $op_assign(&mut self, rhs: Output) {
                *self = self.clone().$op(rhs);
            }
        }

        impl $OpAssign<&Output> for Output {
            fn $op_assign(&mut self, rhs: &Output) {
                *self = self.clone().$op(rhs);
            }
        }
    };
}

cwise_binary_op!(AddAssign, add_assign, Add, add, |x: Output, y: Output| {
    let graph = x.graph().clone();

    /*
    if graph.nodes.get(&x.node_id).unwrap().operation.is_zero() {
        return y;
    }
    if graph.nodes.get(&y.node_id).unwrap().operation.is_zero() {
        return x;
    }
    */

    graph
        .add_node(
            None,
            AddOp {
                input_specs: vec![x.spec(), y.spec()],
            },
            &[x, y],
        )
        .remove(0)
});

cwise_binary_op!(MulAssign, mul_assign, Mul, mul, |x: Output, y: Output| {
    let graph = x.graph().clone();

    /*
    let x_op = &graph.get_node(x.key().node_id).unwrap().operation();
    let y_op = &graph.get_node(y.key().node_id).unwrap().operation();

    // TODO: These types of things need to be evaluated post constant folding.
    if x_op.is_one() {
        return y;
    }
    if y_op.is_one() {
        return x;
    }
    if x_op.is_zero() {
        return x;
    }
    if y_op.is_zero() {
        return y;
    }
    */

    graph
        .add_node(
            None,
            MulOp {
                input_specs: vec![x.spec(), y.spec()],
            },
            &[x, y],
        )
        .remove(0)
});

cwise_binary_op!(DivAssign, div_assign, Div, div, |x: Output, y: Output| {
    let graph = x.graph().clone();
    graph
        .add_node(
            None,
            DivOp {
                input_specs: vec![x.spec(), y.spec()],
            },
            &[x, y],
        )
        .remove(0)
});

cwise_binary_op!(SubAssign, sub_assign, Sub, sub, |x: Output, y: Output| {
    x + (-1 * y)
});

impl Neg for Output {
    type Output = Output;

    fn neg(self) -> Self::Output {
        -1 * self
    }
}

impl Neg for &Output {
    type Output = Output;

    fn neg(self) -> Self::Output {
        -1 * self
    }
}

// TODO: Convert the input type to an iterator and optimize some of the callers.
pub fn cwise_sum(values: &[Output]) -> Output {
    // TODO: Need to support providing a dtype for this.
    if values.is_empty() {
        return constant(0.0f32);
    }

    let mut current_sum = values[0].clone();
    for val in &values[1..] {
        current_sum += val;
    }

    current_sum
}

pub fn cwise_product(values: &[Output]) -> Output {
    let mut i = values[0].clone();
    for j in values[1..].iter().cloned() {
        i = i * j;
    }

    i
}

pub fn matmul(x: Output, y: Output) -> Output {
    let graph = x.graph().clone();
    graph
        .add_node(
            None,
            MatMulOp {
                input_specs: vec![x.spec(), y.spec()],
            },
            &[x, y],
        )
        .remove(0)
}
