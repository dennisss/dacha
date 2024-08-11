use std::collections::HashMap;
use std::sync::Arc;
use std::{any::Any, collections::HashSet};

use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor::channel::spsc::{self, Receiver, Sender};

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

            let future = async move { op.execute(inputs, outputs).await };

            if target_nodes.contains(&node.name) {
                targets_bundle.add(node_name, future);
            } else {
                bundle.add(node_name, future);
            }
        }

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

// pub struct

#[derive(Clone, Debug)]
pub struct OperationSignature {
    pub name: String,
    pub num_inputs: usize,
    pub num_outputs: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OutputKey {
    pub node_name: String,
    pub output_index: usize,
}

#[async_trait]
pub trait Operation: 'static + Send + Sync {
    fn signature(&self) -> OperationSignature;

    /// Executes the operation on the given streams of inputs and writes the
    /// results to the given output streams.
    ///
    /// Generally this may be called many times in parallel if many separate
    /// graph executions are required.
    ///
    /// Cancellation model:
    /// - Operations that take >=1 inputs MUST terminate shortly after all input
    ///   streams end/close.
    /// - Operations that only have outputs SHOULD stop themselves when the
    ///   output stream is closed (though this isn't necessary for graph
    ///   correctness).
    /// - When the final graph output streams/nodes end (or the caller loses
    ///   interest in them), all operations will abruptly stop getting polled
    ///   (there is no graceful shutdown).
    async fn execute(&self, inputs: Vec<InputStream>, outputs: Vec<OutputStream>) -> Result<()>;
}

pub struct InputStream {
    receiver: Receiver<Box<dyn Any + Send + Sync + 'static>>,
}

impl InputStream {
    /// TODO: Need to differentiate the end of inputs and the stream being fully
    /// consumed.
    pub async fn read(&mut self) -> Option<Box<dyn Any + Send + Sync + 'static>> {
        match self.receiver.recv().await {
            Ok(v) => Some(v),
            Err(_) => None,
        }
    }
}

pub struct OutputStream {
    sender: Sender<Box<dyn Any + Send + Sync + 'static>>,
}

impl OutputStream {
    /// Writes a single packet/frame of data to the stream. This will block if
    /// the unprocessed packet queue is full.
    ///
    /// Returns false if the other end of the stream has been dropped.
    pub async fn write(&mut self, data: Box<dyn Any + Send + Sync + 'static>) -> bool {
        self.sender.send(data).await.is_ok()
    }
}
