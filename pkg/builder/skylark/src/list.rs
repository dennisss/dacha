use std::sync::Mutex;
use std::sync::MutexGuard;

use common::errors::*;
use crypto::hasher::Hasher;

use crate::object::*;
use crate::tuple::*;
use crate::value::*;
use crate::value_attributes;

pub struct ListValue {
    state: Mutex<ListValueState>,
}

struct ListValueState {
    frozen: bool,
    num_iterators: usize,
    elements: Vec<ObjectWeak<dyn Value>>,
}

impl ListValue {
    pub fn new(elements: Vec<ObjectWeak<dyn Value>>) -> Self {
        Self {
            state: Mutex::new(ListValueState {
                elements,
                num_iterators: 0,
                frozen: false,
            }),
        }
    }

    pub fn iter<'a>(&'a self) -> ListValueExclusiveIterator<'a> {
        let state = self.state.lock().unwrap();
        ListValueExclusiveIterator { state, index: 0 }
    }
}

impl Value for ListValue {
    value_attributes!(Mutable | ReprAsStr);

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        let mut state = self.state.lock().unwrap();
        out.extend_from_slice(&state.elements);
    }

    fn freeze_value(&self) {
        let mut state = self.state.lock().unwrap();
        state.frozen = true;
    }

    fn call_bool(&self) -> bool {
        let state = self.state.lock().unwrap();
        !state.elements.is_empty()
    }

    fn call_repr(&self, context: &mut ValueCallFrame) -> Result<String> {
        let state = self.state.lock().unwrap();
        TupleValue::call_repr_impl("[", &state.elements, "]", context)
    }

    fn call_eq(&self, other: &dyn Value, context: &mut ValueCallFrame) -> Result<bool> {
        if core::ptr::eq::<dyn Value>(self, other) {
            return Ok(true);
        }

        let other = match other.as_any().downcast_ref::<Self>() {
            Some(v) => v,
            None => return Ok(false),
        };

        // The other list must be in the stack to ensure we can safely lock the
        // elements.
        let mut context = context.child(other)?;

        let state = self.state.lock().unwrap();
        let other_state = other.state.lock().unwrap();

        TupleValue::call_eq_impl(&state.elements, &other_state.elements, &mut context)
    }

    fn call_iter(&self, frame: &mut ValueCallFrame) -> Result<ObjectStrong<dyn Value>> {
        // TODO: Increment num_iterators

        Err(err_msg("Value not iterable"))
    }

    fn call_len(&self, frame: &mut ValueCallFrame) -> Result<usize> {
        let state = self.state.lock().unwrap();
        Ok(state.elements.len())
    }
}

pub struct ListValueExclusiveIterator<'a> {
    state: MutexGuard<'a, ListValueState>,
    index: usize,
}

impl Iterator for ListValueExclusiveIterator<'_> {
    type Item = ObjectWeak<dyn Value>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(value) = self.state.elements.get(self.index) {
            self.index += 1;
            Some(value.clone())
        } else {
            None
        }
    }
}

pub struct ListValueIterator {
    instance: ObjectWeak<dyn Value>,
    next_index: usize,
}

// impl List

impl ListValueIterator {}
