use std::sync::Arc;
use std::collections::HashMap;

use crate::v2::types::*;

const DEFAULT_PRIORITY: u8 = 16 - 1;

/*
Default priority is 16

Default parent is 0

Weight is 1-256



PRIORITY {
    exclusive: bool,
    dependency_id: StreamId,
    weight: u8
}

could also be in a HEADERS frame

minimally should have on priority entry per stream on which we are still sending data.


When trying to send data:
- 


A
   dependency on a stream that is not currently in the tree -- such as a
   stream in the "idle" state -- results in that stream being given a
   default priority
*/

pub struct PriorityTree {
    root: PriorityTreeNode,
    index: HashMap<StreamId, PriorityTreeNode>
}

impl PriorityTree {
    pub fn new() -> Self {
        Self {
            root: PriorityTreeNode {
               weight: DEFAULT_PRIORITY,
               parent: 0,
               children: vec![] 
            },
            index: HashMap::new()
        }
    }

    pub fn set(&mut self, stream_id: StreamId, weight: u8, dependency_id: StreamId) {



    }

    /// Traverse the tree first emitting ids for streams with no dependencies
    pub fn traverse(&self) {

    }

}


struct PriorityTreeNode {
    weight: u8,
    parent: StreamId,
    children: Vec<StreamId>
}