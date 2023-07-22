use alloc::vec::Vec;
use std::collections::{HashMap, HashSet};

use common::errors::*;

use crate::graph::graph::*;
use crate::graph::tensor::*;

pub struct OperationExecuteContext {
    inputs: Vec<Option<Tensor>>,
    outputs: Vec<Option<Tensor>>,
}

impl OperationExecuteContext {
    /// NOTE: A value can only be retrieved once
    pub fn get_input(&mut self, index: usize) -> Result<Tensor> {
        self.inputs
            .get_mut(index)
            .and_then(|v| v.take())
            .ok_or_else(|| format_err!("No value for input {}", index))
    }

    /// Gets the value of an input which contains a Tensor shape as its value.
    pub fn get_shape_input(&mut self, index: usize) -> Result<Vec<usize>> {
        // TODO: Check that the tensor is 1D
        Ok(self
            .get_input(index)?
            .array::<u32>()
            .unwrap()
            .flat()
            .iter()
            .map(|v| *v as usize)
            .collect::<Vec<_>>())
    }

    pub fn set_output<T: Into<Tensor>>(&mut self, index: usize, value: T) {
        if self.outputs.len() < index + 1 {
            self.outputs.resize(index + 1, None);
        }

        self.outputs[index] = Some(value.into());
    }
}

/// Set of intermediate/final results computed during execution of the compute
/// graph. It's main purpose is to keep track of which intermediate values are
/// still needed by the execution engine and to free obsolete ones.
#[derive(Default)]
struct ExecutionResults {
    tensors: HashMap<OutputKey, ExecutionResult>,
}

#[derive(Default)]
struct ExecutionResult {
    value: Option<Tensor>,

    /// Number of remaining operations that need a copy of this tensor.
    references: usize,
}

impl ExecutionResults {
    fn request_value(&mut self, key: OutputKey) {
        self.tensors.entry(key).or_default().references += 1;
    }

    fn set_value(&mut self, key: OutputKey, value: Tensor) {
        let entry = match self.tensors.get_mut(&key) {
            Some(v) => v,
            // No one wants this value so immedatiely discard.
            None => return,
        };

        entry.value = Some(value);
    }

    fn has_value(&self, key: OutputKey) -> bool {
        self.tensors
            .get(&key)
            .map(|v| v.value.is_some())
            .unwrap_or(false)
    }

    fn take(&mut self, key: OutputKey) -> Option<Tensor> {
        let entry = match self.tensors.get_mut(&key) {
            Some(v) => v,
            None => return None,
        };

        entry.references -= 1;

        let value = entry.value.clone();

        if entry.references == 0 {
            self.tensors.remove(&key);
        }

        value
    }
}

pub async fn execute<I: Iterator<Item = (Output, Tensor)>>(
    inputs: I,
    outputs: &[Output],
) -> Result<Vec<Tensor>> {
    if outputs.is_empty() {
        return Ok(vec![]);
    }

    let graph = outputs[0].graph().clone();

    // TODO: Verify that they are all in the same graph.

    // TODO: Once we no longer care about a computed value, we should delete it (or
    // re-use the buffers). ^ Should prioritize deleting tensors before we
    // execute more stuff. -> Other issue

    let mut results = ExecutionResults::default();
    let mut input_keys = vec![];
    for (input, input_value) in inputs {
        if results.has_value(input.key()) {
            return Err(err_msg("Duplicate input fed"));
        }

        // TODO: Check that the inputs match the requested spec.

        results.request_value(input.key());
        results.set_value(input.key(), input_value.clone());
        input_keys.push(input.key());
    }

    // Nodes that can be immediately executed (because all inputs have been
    // computed).
    let mut schedulable = vec![];

    let mut scheduled_set = HashSet::new();

    // Map from a node A to all other nodes B such that node B depends on node A to
    // finish before executing. Only includes B nodes that need to be executed.
    let mut dependants = HashMap::<NodeId, Vec<NodeId>>::new();

    // Run DFS to find all nodes that need to be executed.
    {
        // These will contain the set of nodes that we want to execute to produce the
        // outputs.
        let mut visited = HashSet::new();
        let mut stack = vec![];

        for output in outputs.iter().cloned() {
            results.request_value(output.key());

            // Skip if the output value was already provided as an input value.
            if results.has_value(output.key()) {
                continue;
            }

            // Skip if node is already being visited.
            if !visited.insert(output.key().node_id()) {
                continue;
            }

            stack.push(output.key().node_id());
        }

        while let Some(node_id) = stack.pop() {
            let node = graph.get_node(node_id).unwrap();

            let mut all_inputs_ready = true;
            for input_key in node.inputs.iter().cloned() {
                results.request_value(input_key);

                if results.has_value(input_key) {
                    continue;
                }

                if visited.insert(input_key.node_id()) {
                    stack.push(input_key.node_id());
                }

                dependants
                    .entry(input_key.node_id())
                    .or_default()
                    .push(node_id);

                all_inputs_ready = false;
            }

            if all_inputs_ready {
                schedulable.push(node_id);
                scheduled_set.insert(node_id);
            }
        }
    }

    // Decrement the refcount for input tensors now that all references are setup.
    for input_key in input_keys {
        results.take(input_key);
    }

    /*
    TODO: We don't necessarily want to parallelize every operation:
    - If two operations reference the same tensor and one can re-use the tensor's input buffer, then it is best to run that one last.

    TODO: If there are any super fast ops like shape() we should prioritize running those before other dependencies.
    - This will potentially allow re-use of buffers by other more expensive ops.
    - Could be implemented by adding control dependencies in an optimizer pass over the graph.

    TODO: Must discard tensors once they are no longer needed.
    - could keep a counter of how many times an output is referenced.
    - If not referenced at all, then we

    */

    while let Some(node_id) = schedulable.pop() {
        let node = graph.get_node(node_id).unwrap();

        let mut ctx = OperationExecuteContext {
            inputs: vec![],
            outputs: vec![],
        };

        for input_key in &node.inputs {
            let value = results
                .take(*input_key)
                .ok_or_else(|| err_msg("Input not available for executing node"))?;
            ctx.inputs.push(Some(value));
        }

        ctx.outputs
            .resize(node.operation().signature().outputs.len(), None);

        node.operation().execute(&mut ctx).await?;

        for (i, value) in ctx.outputs.into_iter().enumerate() {
            let v = value
                .as_ref()
                .take()
                .cloned()
                .ok_or_else(|| err_msg("No output value produced"))?;
            // TODO: Verify it doesn't already exist.

            let k = OutputKey {
                node_id,
                output_index: i as u32,
            };

            results.set_value(k, v);
        }

        // Attempt to schedule more nodes to execute.
        for node_id in dependants.remove(&node_id).unwrap_or_default() {
            let node = graph.get_node(node_id).unwrap();
            let mut all_inputs_ready = true;
            for input_key in node.inputs.iter().cloned() {
                if results.has_value(input_key) {
                    continue;
                }

                all_inputs_ready = false;
            }

            if all_inputs_ready && scheduled_set.insert(node_id) {
                schedulable.push(node_id);
            }
        }
    }

    let mut final_outputs = vec![];
    for output in outputs {
        final_outputs.push(
            results
                .take(output.key())
                .ok_or_else(|| err_msg("Missing final value for requested output"))?,
        );
    }

    Ok(final_outputs)
}
