extern crate common;
extern crate executor;
extern crate math_compute;
#[macro_use]
extern crate macros;

use std::collections::{HashMap, HashSet};

use common::errors::*;
use math::array::Array;
use math_compute::*;

/*
gradient of a sum

sum = x1 + x2 + x3
dsum/dx = dx1/dx + ...

Define a function()
    => Allowed to call it assuming we are also building a function

f.evaluate({  })


Requirements for tracing:
- Whenever a module is built, trace its name.
- Whenever a module has functions being added, trace them.

pub struct Module {}

Need to add helpers for building functions

At build time, each module must be told its full name

Then we add a macro to

For things like dropout, need a global indicator of whether or not we are in training or inference model

*/

/*
Goal for Iris dataset
- Input Features
    - sepal length
    - sepal width
    - petal length
    - petal width
- Outputs
    - 3 classes
- Steps
    - Read CSV into 5 tensors
    - Randomly shuffle the dataset.
    - Split into 80/20 training/test
    - Convert into arrays
        - Normalize the float features
        - Convert the labels using a label map.
    - Run through network
        - One-hot encode the label index
        - 2 (8 unit) layers using ReLU
        - 1 (3 unit) layer using Softmax
    - Train network
    - Evaluate using argmax


Goal for MNIST:
- Input: 28x28 grayscale images.
    - Normalize to 0-1 f32
- Output:
    - 10 f32 probabilities
    - MSE (or cross entropy)

- 2 layer NN. 300 hidden units
    -

Activation functions to support
- Sigmoid
- Tangent
- Softmax
- ReLU
- Swish


But, first thing I need to matmul
- W x
- where 'W' is 2d


*/

pub struct CompositeModel {
    regressor: LinearRegressionModel,
}

impl CompositeModel {
    pub fn evaluate(&self, x: Output) -> Result<Output> {
        Ok(self.regressor.regress(x))
    }
}

pub trait RegressionModel {
    fn regress(&self, x: Output) -> Output;
}

pub trait HParams<T> {
    fn build(&self, group: Group) -> Result<T>;
}

pub struct Linear {
    group: Group,
    w: Output,
    b: Output,
}

pub struct LinearHParams {
    pub input_features: usize,
    pub output_features: usize,
}

impl Linear {
    pub fn run(&self, x: Output) -> Output {
        // TODO: Challenge is that its only meaningful to call run() once. After that,
        // additional invocations will need to duplicate operations in the same context.
        let ctx = self.group.context();

        // 'x' is of shape [batch_size, num_features]
        // need to reshape to [batch_size, num_features, 1]
        let x = x.expand_dims(&[-1]);

        matmul(self.w.clone(), x).squeeze(&[-1]) + &self.b
    }
}

impl HParams<Linear> for LinearHParams {
    fn build(&self, group: Group) -> Result<Linear> {
        let ctx = group.context();
        let w = variable(
            "w",
            Array::<f32>::zeros(&[self.output_features, self.input_features]),
        );
        let b = variable("b", Array::<f32>::zeros(&[self.output_features]));

        drop(ctx);
        Ok(Linear { group, w, b })
    }
}

pub struct LinearRegressionHParams {
    /// Number of dimensions in the inputs.
    pub dims: usize,
}

// Annotate as a 'module'
pub struct LinearRegressionModel {
    group: Group,
    w: Output,
    b: Output,
}

impl RegressionModel for LinearRegressionModel {
    fn regress(&self, x: Output) -> Output {
        let ctx: GroupContext = self.group.context();
        return (&self.w * x) + &self.b;
    }
}

impl HParams<LinearRegressionModel> for LinearRegressionHParams {
    fn build(&self, group: Group) -> Result<LinearRegressionModel> {
        let ctx = group.context();
        let w = variable("w", Array::<f32>::zeros(&[self.dims]));
        let b = variable("b", Array::<f32>::zeros(&[self.dims]));

        drop(ctx);
        Ok(LinearRegressionModel { group, w, b })
    }
}

pub struct Trainer {
    loss: Output,
    learning_rate: Output,
    state: Vec<(Output, Tensor)>,
    next_state: Vec<Output>,
}

impl Trainer {
    pub fn create(loss: Output) -> Result<Self> {
        // Find all variables and initialize them

        let mut state = vec![];

        // DFS
        {
            let mut stack = vec![];
            let mut visited = HashSet::new();

            stack.push(loss.key().node_id());
            visited.insert(loss.key().node_id());

            while let Some(node_id) = stack.pop() {
                let node = loss.graph().get_node(node_id).unwrap();

                if let Some(op) = node.operation().as_any().downcast_ref::<InputOp>() {
                    if op.spec.trainable {
                        state.push((
                            Output::from_parts(
                                loss.graph().clone(),
                                OutputKey {
                                    node_id,
                                    output_index: 0,
                                },
                            ),
                            op.spec.initial_value.clone().unwrap(),
                        ));
                    }
                }

                for input_key in node.inputs() {
                    if visited.insert(input_key.node_id()) {
                        stack.push(input_key.node_id());
                    }
                }
            }
        }

        let mut differ = ReverseDifferentiator::new(loss.graph().clone());

        // TODO: Replace with a step index.
        let learning_rate = input(DataType::Float32);

        let mut next_state = vec![];
        for (x, _) in &state {
            let dx = differ.gradient(loss.clone(), x.clone())?;

            // TODO: Ensure the shapes of dx and x are compatible

            let x_next = (&learning_rate * dx) + x;
            next_state.push(x_next);
        }

        Ok(Self {
            loss,
            learning_rate,
            state,
            next_state,
        })
    }

    pub async fn train_step(&mut self, inputs: &[(Output, Tensor)]) -> Result<()> {
        let mut merged_inputs = vec![];

        merged_inputs.push((
            self.learning_rate.clone(),
            Array::<f32>::scalar(-0.001).into(),
        ));

        merged_inputs.extend(inputs.iter().cloned());
        // TODO: Allow re-using these buffers.
        merged_inputs.extend(self.state.iter().cloned());

        // TODO: Rename to targets.
        let mut outputs = vec![];
        outputs.push(self.loss.clone());
        outputs.extend(self.next_state.iter().cloned());

        let results = execute(merged_inputs.into_iter(), &outputs).await?;
        println!("Loss: {:?}", results[0]);

        for i in 0..self.state.len() {
            let key = self.state[i].0.clone();
            let value = results[i + 1].clone();
            self.state[i] = (key, value);
        }

        Ok(())
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let mut graph = Graph::new();
    let mut graph_ctx = GraphContext::new(graph.clone());

    /*
    let data = math_compute::io::MNISTDataset::load().await?;
    println!("{:?}", data.test_images.shape);
    println!("{:?}", data.test_labels.shape);
    return Ok(());
    */

    let model = LinearHParams {
        input_features: 1,
        output_features: 1,
    }
    .build(graph.subgroup("model"))?;

    let x = input(DataType::Float32);
    let y_target = input(DataType::Float32);

    let loss = {
        let group = graph.subgroup("loss");
        let ctx = group.context();

        math_compute::mean_squared_error(
            model.run(x.clone()).squeeze(&[-1]),
            y_target.squeeze(&[-1]),
        )
    };

    let mut trainer = Trainer::create(loss)?;

    let x_data =
        Tensor::from(Array::<f32>::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).expand_dims(&[-1]));
    let y_data = x_data.clone();

    for i in 0..20 {
        trainer
            .train_step(&[
                (x.clone(), x_data.clone()),
                (y_target.clone(), y_data.clone()),
            ])
            .await?;
    }

    println!("{:?}", trainer.state);

    // let loss_fn = mean_squared_error(model.regress(x), y2)

    Ok(())
}
