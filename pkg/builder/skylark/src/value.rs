use core::any::Any;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Mutex;
use std::sync::{Arc, Weak};

use common::any::AsAny;
use common::errors::*;
use crypto::hasher::Hasher;
use crypto::sip::SipHasher;

use crate::object::*;

const MAX_CALL_STACK_SIZE: usize = 100;

/*
TODO: Double check everything is consistent with:
https://bazel.build/rules/language#differences_with_python

TODO: Optimize an Arc<String> down to a single pointer.
TODO: If a value is a primitive (can't contain other values), we don't need to store any parent pointer information.
TODO: If the value is a primitive, directly store it in the ValuePtr rather than using indirection?
*/

pub trait Value: 'static + AsAny {
    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>);

    /// Should make the contents of the value completely immutable.
    fn freeze_value(&self);

    /// Evalutes this value as a boolean. Used to implement 'bool(X)'
    fn call_bool(&self) -> bool;

    fn call_repr(&self, frame: &mut ValueCallFrame) -> Result<String>;

    /// Returns the string you'd get be calling str(X) on this value.
    fn call_str(&self, frame: &mut ValueCallFrame) -> Result<String>;

    /// Calls __hash__. Note that only immutable types should implement this.
    fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()>;

    fn call_eq(&self, other: &dyn Value, frame: &mut ValueCallFrame) -> Result<bool>;

    fn call_iter(&self, frame: &mut ValueCallFrame) -> Result<ObjectStrong<dyn Value>> {
        Err(err_msg("Value not iterable"))
    }

    fn call_next(&self, frame: &mut ValueCallFrame) -> Result<ObjectStrong<dyn Value>> {
        Err(err_msg("Value is not an iterator"))
    }

    fn call_len(&self, frame: &mut ValueCallFrame) -> Result<usize> {
        Err(err_msg("Value has no length"))
    }

    fn call_add(
        &self,
        other: &dyn Value,
        frame: &mut ValueCallFrame,
    ) -> Result<ObjectStrong<dyn Value>> {
        Err(err_msg("Value has no length"))
    }
}

impl Object for dyn Value {
    fn freeze_object(&self) {
        self.freeze_value();
    }

    fn referenced_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        self.referenced_value_objects(out)
    }
}

/// Helper macro to define common methods for types that implement 'Value'.
#[macro_export]
macro_rules! value_attributes {
    ($first:ident | $($rest:ident)|*) => {
        value_attributes!($first);
        $(
            value_attributes!($rest);
        )*
    };
    (Immutable) => {
        fn freeze_value(&self) {}
    };
    (Mutable) => {
        fn call_hash(&self, hasher: &mut dyn Hasher, frame: &mut ValueCallFrame) -> Result<()> {
            Err(err_msg("Can not reliably hash mutable value."))
        }
    };
    (NoChildren) => {
        fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {}
    };
    (ReprAsStr) => {
        fn call_str(&self, frame: &mut ValueCallFrame) -> Result<String> {
            self.call_repr(frame)
        }
    };
}

/// Context/frame provided when calling a native method on a Value type.
///
/// This is used on methods of the Value trait which have a well defined
/// signature composed of native types.
///
/// Some notes:
/// - Calls are not allowed to be recursive (reference the same value twice in
///   the call stack).
/// - A ValueCallFrame must outlive the duration of method calls on the
///   associated value.
pub struct ValueCallFrame<'a> {
    instance: Option<&'a dyn Value>,

    pool: &'a ObjectPool<dyn Value>,

    parent_pointers: &'a mut ValuePointers,
}

impl<'a> Drop for ValueCallFrame<'a> {
    fn drop(&mut self) {
        if let Some(inst) = &self.instance {
            self.parent_pointers.remove(*inst);
        }
    }
}

impl<'a> ValueCallFrame<'a> {
    /// Creates a new calling context associated with no value.
    ///
    /// This should only be run once in the runtime when a source code file
    /// starts being evaluated.
    ///
    /// DO NOT pass a root context to methods. Instead always pass a context
    /// associated with the value being called by creating one with
    /// .child_context.
    pub fn root(pool: &'a ObjectPool<dyn Value>, parent_pointers: &'a mut ValuePointers) -> Self {
        Self {
            instance: None,
            pool,
            parent_pointers,
        }
    }

    pub fn pool(&self) -> &ObjectPool<dyn Value> {
        &self.pool
    }

    /// Creates a child frame associated with a given value.
    /// - This internally gurantees that there is no recursion (no parent
    ///   context refers to the same value).
    /// - The return value of this can be passed to methods of 'value'. Note
    ///   that methods of 'value' can only expect 'self' to be guranteed to not
    ///   be recursing and nothing is implied about other arguments passed to
    ///   the method.
    pub fn child<'b>(&'b mut self, value: &'b dyn Value) -> Result<ValueCallFrame<'b>> {
        if self.parent_pointers.pointers.len() >= MAX_CALL_STACK_SIZE {
            return Err(err_msg("Exceeded max call stack depth"));
        }

        if !self.parent_pointers.insert(value) {
            return Err(err_msg("Recursion detected"));
        }

        Ok(ValueCallFrame {
            instance: Some(value),
            pool: self.pool,
            parent_pointers: self.parent_pointers,
        })
    }

    /// Standard method for getting a hasher instance.
    pub fn new_hasher(&self) -> SipHasher {
        // NOTE: We care more about determinism than performance or security.
        SipHasher::default_rounds_with_key_halves(0, 0)
    }
}

#[derive(Default)]
pub struct ValuePointers {
    pointers: HashSet<*const dyn Value>,
}

impl ValuePointers {
    pub fn insert(&mut self, value: &dyn Value) -> bool {
        self.pointers.insert(value as *const dyn Value)
    }

    pub fn remove(&mut self, value: &dyn Value) {
        self.pointers.remove(&(value as *const dyn Value));
    }
}

/*
/// Pre-allocated
pub struct ValueCache {}

/// Wrapper around an ObjectPool<dyn Value> which also re-uses references to frequently created values.
pub struct ValuePool {
    cache: Arc<ValueCache>,
}
*/
