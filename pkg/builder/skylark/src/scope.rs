use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use common::errors::*;

use crate::dict::DictValue;
use crate::object::ObjectPool;
use crate::object::ObjectStrong;
use crate::primitives::StringValue;
use crate::value::Value;
use crate::value::ValueCallFrame;

/// TODO: Implement a method of blocking recursive scopes.
/// - Simply speaking every function is a Value bound to some instance and we
///   can't see the same Value in the stack space.
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

    pub fn resolve(
        &self,
        name: &str,
        context: &mut ValueCallFrame,
    ) -> Result<Option<ObjectStrong<dyn Value>>> {
        {
            let mut inner_context = context.child(self.bindings())?;

            if let Some(value) = self
                .bindings()
                .get(&StringValue::new(name.to_string()), &mut inner_context)?
            {
                return Ok(Some(value));
            }
        }

        if let Some(parent) = &self.parent {
            return parent.resolve(name, context);
        }

        Ok(None)
    }

    pub fn bindings(&self) -> &DictValue {
        &self.bindings.as_any().downcast_ref().unwrap()
    }
}
