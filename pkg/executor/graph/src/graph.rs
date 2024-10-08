use std::collections::HashMap;
use std::sync::Arc;
use std::{any::Any, collections::HashSet};

use base_error::*;
use executor::bundle::TaskResultBundle;
use executor::channel::spsc::{self, Receiver, Sender};

use crate::operation::*;
use crate::stream::*;

#[derive(Default)]
pub struct Graph {
    nodes: HashMap<String, Node>,
}

impl Graph {
    pub fn add_node(&mut self, name: &str, operation: Arc<dyn Operation>, inputs: &[OutputKey]) {
        self.nodes.insert(
            name.to_string(),
            Node {
                name: name.to_string(),
                operation,
                inputs: inputs.to_vec(),
            },
        );
    }

    pub async fn execute(&self, target_nodes: Vec<String>) -> Result<()> {
        // Run DFS to find the full set of nodes we need to execute.
        let mut nodes_to_run: HashSet<&str> = HashSet::default();
        {
            let mut pending = vec![];
            for node in &target_nodes {
                if nodes_to_run.contains(node.as_str()) {
                    continue;
                }

                pending.push(node.as_str());
                nodes_to_run.insert(node.as_str());
            }

            while let Some(node_name) = pending.pop() {
                let node = self
                    .nodes
                    .get(node_name)
                    .ok_or_else(|| format_err!("No node named: {}", node_name))?;

                for input in &node.inputs {
                    if nodes_to_run.contains(input.node_name.as_str()) {
                        continue;
                    }

                    nodes_to_run.insert(input.node_name.as_str());
                    pending.push(input.node_name.as_str());
                }
            }
        }

        // Generate all channels for sending outputs to inputs.
        let mut node_inputs = HashMap::new();
        let mut node_outputs = HashMap::new();
        for node_name in nodes_to_run.iter().cloned() {
            let node = self
                .nodes
                .get(node_name)
                .ok_or_else(|| format_err!("No node named: {}", node_name))?;

            let mut outputs = vec![];
            for output_index in 0..node.operation.signature().num_outputs {
                let key = OutputKey {
                    node_name: node_name.to_string(),
                    output_index,
                };

                let (input, output) = Self::new_io_stream();

                node_inputs.insert(key.clone(), input);
                outputs.push(output);
            }

            node_outputs.insert(node_name, outputs);

            // TODO: Also flag at this point how many nodes require each output
            // as an input.
        }

        // Start all nodes.
        let mut bundle = TaskResultBundle::new();
        let mut targets_bundle = TaskResultBundle::new();
        for node_name in nodes_to_run {
            let node = self
                .nodes
                .get(node_name)
                .ok_or_else(|| format_err!("No node named: {}", node_name))?;

            // TODO: Support multiple nodes pulling from the same output stream of another
            // node.
            let mut inputs = vec![];
            for input_key in &node.inputs {
                let input_stream = node_inputs
                    .remove(input_key)
                    .ok_or_else(|| err_msg("Unknown input to node"))?;
                inputs.push(input_stream);
            }

            let mut outputs = node_outputs.remove(&node_name).unwrap();

            let op = node.operation.clone();

            // TODO: Catch GraphStreamError.
            // TODO: Must close the output screws eventually.
            let future = async move { op.execute(inputs, outputs).await };

            if target_nodes.contains(&node.name) {
                targets_bundle.add(node_name, future);
            } else {
                bundle.add(node_name, future);
            }
        }

        // TODO: Need stall detection: If all operations are stuck waiting for more
        // input (e.g. if an op can't consume an input until some other op consumes all
        // of that same input).

        // TODO: Also stop this if any thing in 'bundle' returns an error.
        targets_bundle.join().await?;

        Ok(())
    }

    fn new_io_stream() -> (InputStream, OutputStream) {
        let (sender, receiver) = spsc::bounded(8);
        let input = InputStream { receiver };
        let output = OutputStream { sender };
        (input, output)
    }
}

pub struct Node {
    name: String,
    operation: Arc<dyn Operation>,
    inputs: Vec<OutputKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OutputKey {
    pub node_name: String,
    pub output_index: usize,
}
