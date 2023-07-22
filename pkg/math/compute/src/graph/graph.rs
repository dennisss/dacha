use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt::Debug;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::graph::operation::{Operation, TensorSpec};
use crate::DataType;

// TODO: Switch to being task local.
#[thread_local]
static mut CURRENT_GRAPH: Option<Graph> = None;

/// A collection of computation nodes.
///
/// Modifying existing nodes requires mutable access to the graph, but strictly
/// appending nodes can occur only any access.
///
/// Nodes are stored in a Graph primarily as a memory optimization (to allow
/// nodes to be arena allocated in the future), but the graph doesn't constrain
/// which subset of sub-graphs will actually be computed.
#[derive(Debug, Clone)]
pub struct Graph {
    state: Arc<Mutex<GraphState>>,
}

#[derive(Debug)]
struct GraphState {
    // TODO: Arena or slab allocate all of these nodes.
    nodes: HashMap<NodeId, Arc<Node>>,

    last_node_id: NodeId,

    /// Names (and name prefixes) that have already been allocated in this
    /// graph. TODO: Re-use the same memory for this and the 'nodes'
    name_index: HashSet<String>,

    group_stack: Vec<String>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(GraphState {
                nodes: HashMap::new(),
                last_node_id: NodeId(0),
                name_index: HashSet::new(),
                group_stack: vec![],
            })),
        }
    }

    pub fn get_node(&self, node_id: NodeId) -> Option<Arc<Node>> {
        self.state.lock().unwrap().nodes.get(&node_id).cloned()
    }

    /// - 'name' is given is a user requested name for the node. This name must
    ///   not already be allocated.
    pub fn add_node<O: Operation>(
        &self,
        name: Option<&str>,
        operation: O,
        inputs: &[Output],
    ) -> Vec<Output> {
        let sig = operation.signature();
        assert_eq!(inputs.len(), sig.inputs.len());

        let mut input_keys = vec![];
        for input in inputs {
            assert!(self.same_graph_as(&input.graph));
            input_keys.push(input.key);
        }

        let mut state = self.state.lock().unwrap();

        let id = NodeId(state.last_node_id.0 + 1);
        state.last_node_id = id;

        let full_name = {
            let prefix = state.group_stack.last().map(|s| s.as_ref()).unwrap_or("");
            let suffix = name.unwrap_or(sig.name);

            // TODO: Also prevent '/'
            assert!(!suffix.contains("."));

            let mut proposed_name = format!("{}{}", prefix, suffix);

            if name.is_some() {
                assert!(!state.name_index.contains(&proposed_name));
            }

            let original_proposed_name = proposed_name.clone();

            let mut i = 2;
            while state.name_index.contains(&proposed_name) {
                proposed_name = format!("{}_{}", original_proposed_name, i);
                i += 1;
            }

            proposed_name
        };

        state.name_index.insert(full_name.clone());

        state.nodes.insert(
            id,
            Arc::new(Node {
                name: full_name,
                id,
                operation: Box::new(operation),
                inputs: input_keys,
            }),
        );

        let mut outputs = vec![];
        for i in 0..sig.outputs.len() {
            outputs.push(Output {
                key: OutputKey {
                    node_id: id,
                    output_index: i as u32,
                },
                graph: self.clone(),
            });
        }

        outputs
    }

    pub fn same_graph_as(&self, other: &Self) -> bool {
        return core::ptr::eq::<Mutex<GraphState>>(self.state.as_ref(), other.state.as_ref());
    }

    pub fn subgroup(&self, prefix: &str) -> Group {
        assert!(!prefix.contains("."));
        Group::create(self.clone(), prefix)
    }

    pub fn absolute_group(&self, prefix: &str) -> Group {
        Group::create(self.clone(), prefix)
    }

    /*
    General grouping strategy:

    - model.conv[0].kernel
    - model.conv[0].kernel/gradient1/Add

    */
}

pub struct GraphContext {
    hidden: (),
}

// TODO: Re-consider these once we support task local storage.
impl !Sync for GraphContext {}
impl !Send for GraphContext {}

impl GraphContext {
    pub fn new(graph: Graph) -> Self {
        unsafe {
            assert!(CURRENT_GRAPH.is_none());
            CURRENT_GRAPH = Some(graph);
        }
        Self { hidden: () }
    }

    pub(crate) fn current_graph() -> Option<Graph> {
        unsafe { CURRENT_GRAPH.clone() }
    }
}

impl Drop for GraphContext {
    fn drop(&mut self) {
        unsafe { CURRENT_GRAPH.take() };
    }
}

//

/// A collection of similar nodes identified by a name prefix that is given to
/// all nodes in the group.
///
/// For a given graph, it is guaranteed that only one Group exists for a given
/// prefix and can't be cloned.
///
/// TODO: How should we preserve groups when pruning/optimization occurs?
pub struct Group {
    graph: Graph,
    absolute_prefix: String,
}

impl Group {
    fn create<S: Into<String>>(graph: Graph, absolute_prefix: S) -> Group {
        let absolute_prefix = absolute_prefix.into();
        assert!(graph
            .state
            .lock()
            .unwrap()
            .name_index
            .insert(absolute_prefix.clone()));
        Self {
            graph,
            absolute_prefix,
        }
    }

    pub fn subgroup(&self, prefix: &str) -> Group {
        assert!(!prefix.contains("."));
        Self::create(
            self.graph.clone(),
            &format!("{}.{}", self.absolute_prefix, prefix),
        )
    }

    pub fn context(&self) -> GroupContext {
        self.graph
            .state
            .lock()
            .unwrap()
            .group_stack
            .push(format!("{}.", self.absolute_prefix));

        GroupContext { group: self }
    }
}

pub struct GroupContext<'a> {
    group: &'a Group,
}

impl<'a> Drop for GroupContext<'a> {
    fn drop(&mut self) {
        self.group.graph.state.lock().unwrap().group_stack.pop();
    }
}

/// NOTE: Node ids are not meant to be serializable as they should only ever
/// exist temporarily in memory.
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct NodeId(u32);

impl Debug for NodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[Node Id: {}]", self.0)
    }
}

#[derive(Debug)]
pub struct Node {
    name: String,
    id: NodeId,
    operation: Box<dyn Operation>,
    pub(crate) inputs: Vec<OutputKey>,
}

impl Node {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn operation(&self) -> &dyn Operation {
        self.operation.as_ref()
    }

    pub fn inputs(&self) -> &[OutputKey] {
        &self.inputs
    }
}

/// Key used to identify an output produced by a Node after it is executed.
/// Every output in a single graph will have a distinct key.
///
/// TODO: Make all attributed private?
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct OutputKey {
    // TODO: Make more private.
    pub node_id: NodeId,
    pub output_index: u32,
}

impl OutputKey {
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }
}

impl Debug for OutputKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[Node: {}, Idx: {}]", self.node_id.0, self.output_index)
    }
}

/// NOTE: There is intentionally no equality operators on an 'Output' to prevent
/// accidentally attempting to perform comparison of the values of the tensor.
#[derive(Clone)]
pub struct Output {
    graph: Graph,
    key: OutputKey,
}

impl Debug for Output {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let node = self.graph.get_node(self.key.node_id).unwrap();

        write!(f, "{}:{}", node.name, self.key.output_index)
    }
}

impl Output {
    /// NOTE: It is the user's responsibilty to ensure that the key corresponds
    /// to nodes in the given graph.
    pub fn from_parts(graph: Graph, key: OutputKey) -> Self {
        Self { graph, key }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn key(&self) -> OutputKey {
        self.key
    }

    pub fn spec(&self) -> TensorSpec {
        self.graph
            .get_node(self.key.node_id)
            .unwrap()
            .operation()
            .signature()
            .outputs[self.key.output_index as usize]
            .clone()
    }

    pub fn dtype(&self) -> DataType {
        self.spec().dtype
    }
}
