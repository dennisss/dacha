// Code for driving auto-differentiation of the compute graph.

use alloc::vec::Vec;
use std::collections::{HashMap, HashSet};

use common::errors::*;

use crate::graph::*;

/// Builds gradient computations onto an existing graph using reverse
/// auto-differentiation.
pub struct ReverseDifferentiator {
    /// Reference to the graph on which we are performing differentiation.
    ///
    /// We need to know this in order to ensure that our gradient cache is
    /// valid:
    ///
    /// 1. Nodes that we have differentiated must stay immutable for
    /// the lifetime of the differentiator instance for the gradients of those
    /// nodes to be re-usable.
    ///
    /// 2. We don't want to accidentally confuse nodes from different graphs
    /// with the same id when doing cache lookups.
    graph: Graph,

    /// All the that have been computed so far.
    /// We store OutputKeys rather than Output structs primarily to save memory.
    gradients: HashMap<GradientKey, OutputKey>,
}

/// Key (A, B) for a partial derivative of dA/dB.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct GradientKey(OutputKey, OutputKey);

impl ReverseDifferentiator {
    pub fn new(graph: Graph) -> Self {
        Self {
            graph,
            gradients: HashMap::new(),
        }
    }

    /// Derives an expression that computes 'df/dx' (partial derivative of 'f'
    /// w.r.t. 'x'). Both 'f' and 'x' may be multi-dimensional arrays.
    ///
    /// Internally while computing df/dx, this will also compute and cache
    /// expressions for all df/di where 'i' is an intermediate value that
    /// contributes to 'f'. This is mainly for efficiency as it is expected that
    /// the gradient() function will typically be called multiple times for a
    /// given 'f' and different 'x' values.
    pub fn gradient(&mut self, f: Output, x: Output) -> Result<Output> {
        assert!(self.graph.same_graph_as(&f.graph()));
        assert!(self.graph.same_graph_as(&x.graph()));

        let id = GradientKey(f.key(), x.key());

        // Check if already computed.
        if let Some(key) = self.gradients.get(&id) {
            return Ok(Output::from_parts(self.graph.clone(), key.clone()));
        }

        if f.key() == x.key() {
            // dx/dx = 1
            // TODO: Copy the shape/dtype of the existing tensor?
            return Ok(crate::ops::constant(math::array::Array::<f32>::scalar(1.0)));
        }

        // Accumulated partial derivatives for intermediate values.
        // 'dy/di = sum(adjoints[di])' once all nodes after 'i' have been
        // backpropagated.
        let mut adjoints: HashMap<OutputKey, Vec<OutputKey>> = HashMap::new();

        // Seed the first adjoint as the final node at 'f' doesn't have any dependants.
        // TODO: Make sure that this doesn't break any graident code that requires the
        // output shape to be larger than the input shape to ops.
        self.gradients.insert(
            GradientKey(f.key(), f.key()),
            crate::ops::constant(1.0f32).key(),
        );

        let node_ordering = self.topological_subgraph_order(x.clone(), f.clone());

        for node_id in node_ordering {
            let node = self.graph.get_node(node_id).unwrap();

            // Can skip computing these if we have already visited this node when computing
            let op = node.operation().clone();
            let sig = op.signature();

            let outputs = (0..sig.outputs.len())
                .map(|i| {
                    Output::from_parts(
                        self.graph.clone(),
                        OutputKey {
                            node_id,
                            output_index: i as u32,
                        },
                    )
                })
                .collect::<Vec<_>>();

            // For each output of the current node 'o', we should now have a well defined
            // value for 'df/do'.
            for o in outputs.iter() {
                let df_do_key = GradientKey(f.key(), o.key());
                if self.gradients.contains_key(&df_do_key) {
                    continue;
                }

                // This should always be present if our traversal order was correct.
                // The number of elements in this should be equal to the number of direct
                // dependants of 'i' which contribute to 'f'.
                let df_do_parts = adjoints
                    .remove(&o.key())
                    .ok_or_else(|| err_msg("No adjoint values acculated from dependant nodes"))?
                    .into_iter()
                    .map(|key| Output::from_parts(self.graph.clone(), key))
                    .collect::<Vec<_>>();

                self.gradients
                    .insert(df_do_key, crate::ops::cwise_sum(&df_do_parts).key());
            }

            let inputs: Vec<Output> = node
                .inputs
                .iter()
                .map(|key| Output::from_parts(self.graph.clone(), *key))
                .collect::<Vec<_>>();

            // No need to do backpropagation if there are no inputs to the current op.
            if inputs.is_empty() {
                continue;
            }

            // Skip backpropagation if we already did it in the past for this node.
            // (this line makes it fairly efficient to call self.gradient() many times for
            // the same 'f' and works in a pair with the other contains_key statement
            // above).
            if self
                .gradients
                .contains_key(&GradientKey(f.key(), inputs[0].key()))
            {
                continue;
            }

            // Skip backprogation if we already hit the node that we care about.
            if x.key().node_id() == node_id {
                continue;
            }

            let mut df_do_list = vec![];
            for o in outputs.iter() {
                // NOTE: This should always exist as we computed them in the previous loop.
                df_do_list.push(Output::from_parts(
                    self.graph.clone(),
                    *self
                        .gradients
                        .get(&GradientKey(f.key(), o.key()))
                        .ok_or_else(|| err_msg("Missing gradient of output"))?,
                ));
            }

            // Backpropagate gradients of each output to gradients of each input.

            // TODO: Wrap the execution of this in a node group ([node_name.gradient])
            // (though there may be multiple gradients.)

            // TODO: Pick a unique name if multiple gradients are getting computed
            let gradient_group = self
                .graph
                .absolute_group(&format!("{}.gradient", node.name()));

            let ctx = gradient_group.context();
            let partial_df_di_list = op.gradient(OperationGradientContext {
                inputs: &inputs,
                outputs: &outputs,
                doutputs: &df_do_list,
            })?;
            drop(ctx);

            if partial_df_di_list.len() != inputs.len() {
                return Err(err_msg("Incorrect number of gradients computed"));
            }

            for input_idx in 0..inputs.len() {
                let i = inputs[input_idx].key();
                let partial_df_di = partial_df_di_list[input_idx].clone();
                adjoints.entry(i).or_default().push(partial_df_di.key());
            }
        }

        // Grab the final computed gradient.
        // If we didn't compute anything, then 'x' doesn't contribute to 'f' so has a
        // gradient of 0.
        //
        // TODO: Exit early during the planning stage of this function if we detect that
        // 'x' is not in the 'f' graph.
        let df_dx = *self
            .gradients
            .entry(id)
            .or_insert_with(|| crate::ops::constant(0.0f32).key());

        Ok(Output::from_parts(self.graph.clone(), df_dx))
    }

    /// Calculates a topological sorted order for traversing backwards from the
    /// 'output' node back to a leaf node or an occurence of 'input'.
    ///
    /// TODO: Ideally this would prune any paths that don't lead to the 'input'
    fn topological_subgraph_order(&self, input: Output, output: Output) -> Vec<NodeId> {
        let mut ordering = vec![];

        // TODO: Implement stopping at 'input'

        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum NodeState {
            /// Node has never been seen before during graph traversal.
            None,
            /// Node was already seen but its children haven't been expanded.
            Pending,
            /// Node has been visited and we are currently visiting its
            /// children.
            Expanding,
            /// All the node's children have been visited and the node is now in
            /// the 'ordering' list.
            Finalized,
        }

        let mut stack = vec![];
        let mut states = HashMap::new();

        stack.push(output.key().node_id());
        states.insert(output.key().node_id(), NodeState::Pending);

        while let Some(node_id) = stack.last().cloned() {
            let state = states.get(&node_id).cloned().unwrap_or(NodeState::None);
            if state == NodeState::Expanding {
                // We've already expanded this node. So all children of this node should already
                // be in the ordering sort.
                ordering.push(node_id);
                states.insert(node_id, NodeState::Finalized);
                stack.pop();
                continue;
            }

            states.insert(node_id, NodeState::Expanding);

            // Expand all the children
            let node = self.graph.get_node(node_id).unwrap();

            for input_key in &node.inputs {
                let input_state = states
                    .get(&input_key.node_id())
                    .cloned()
                    .unwrap_or(NodeState::None);

                if input_state == NodeState::Expanding {
                    // Cyclic loop in the graph.
                    todo!()
                }

                if input_state != NodeState::None {
                    // Already inserted into stack in a previous iteration.
                    continue;
                }

                stack.push(input_key.node_id());
                states.insert(input_key.node_id(), NodeState::Pending);
            }
        }

        ordering.reverse();

        ordering
    }
}
