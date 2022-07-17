use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use common::errors::*;

use crate::object::ObjectPool;
use crate::object::ObjectStrong;
use crate::value::*;

pub struct Scope {
    source_path: String,

    /// Mapping from variable names to values associated with this.
    /// Always contains a DictValue.
    bindings: ObjectStrong<dyn Value>,

    parent: Option<Arc<Self>>,
}

impl Scope {
    pub fn new(
        source_path: &str,
        pool: &ObjectPool<dyn Value>,
        parent: Option<Arc<Self>>,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            source_path: source_path.to_string(),
            bindings: pool.insert(DictValue::default())?,
            parent,
        }))
    }

    pub fn resolve(&self, name: &str) -> Result<Option<ObjectStrong<dyn Value>>> {
        if let Some(value) = self.bindings().get(name)? {
            return Ok(Some(value));
        }

        if let Some(parent) = &self.parent {
            return parent.resolve(name);
        }

        Ok(None)
    }

    pub fn bindings(&self) -> &DictValue {
        &self.bindings.as_any().downcast_ref().unwrap()
    }
}
