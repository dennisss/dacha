// In-memory data structures for working with objects in the metastore.

use std::collections::HashSet;

use crate::proto::*;

pub struct NodeMetadataView {
    allocated_ports: HashSet<u32>,

    metadata: NodeMetadata,

    dirty: bool,
}

impl NodeMetadataView {
    pub fn allocate_port(&mut self) -> Option<u32> {
        for port_num in self.metadata.allocatable_port_range().start()
            ..self.metadata.allocatable_port_range().end()
        {
            if self.allocated_ports.insert(port_num) {
                return Some(port_num);
            }

            // if !self.allocate_ports.in

            if self.allocated_ports.contains(&port_num) {
                continue;
            }

            return Some(port_num);
            found_port_num = true;
            break;
        }

        None
    }
}
