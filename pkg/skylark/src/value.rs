use core::any::Any;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Mutex;
use std::sync::{Arc, Weak};

use crate::object::*;
use crate::scope::Scope;

use common::any::AsAny;
use common::errors::*;

/*
TODO: Optimize an Arc<String> down to a single pointer.
TODO: If a value is a primitive (can't contain other values), we don't need to store any parent pointer information.
TODO: If the value is a primitive, directly store it in the ValuePtr rather than using indirection?

What is a value has multiple parents?
- Can't change the parent
- Simple solution is to track list of parents
- Other solution is to assume no loops are formed.

- Can we track it through downward sweeping?

*/

pub trait Value: 'static + AsAny {
    /// Evalutes this
    fn test_value(&self) -> bool;

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {}

    /// Should make the contents of the value completely immutable.
    fn freeze_value(&self);

    /// Returns the string you'd get be calling str(X) on this value.
    fn python_str(&self) -> String;
}

impl Object for dyn Value {
    fn freeze_object(&self) {
        self.freeze_value();
    }

    fn referenced_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        self.referenced_value_objects(out)
    }
}

pub trait ValueExt {
    fn downcast_int(&self) -> Option<i64>;

    fn downcast_string(&self) -> Option<&str>;

    fn downcast_float(&self) -> Option<f64>;

    fn downcast_bool(&self) -> Option<bool>;
}

impl ValueExt for dyn Value {
    fn downcast_int(&self) -> Option<i64> {
        self.as_any().downcast_ref::<IntValue>().map(|v| v.value)
    }

    fn downcast_string(&self) -> Option<&str> {
        self.as_any()
            .downcast_ref::<StringValue>()
            .map(|v| v.value.as_str())
    }

    fn downcast_float(&self) -> Option<f64> {
        self.as_any().downcast_ref::<FloatValue>().map(|v| v.value)
    }

    fn downcast_bool(&self) -> Option<bool> {
        self.as_any().downcast_ref::<BoolValue>().map(|v| v.value)
    }
}

pub struct NoneValue {
    hidden: (),
}

impl NoneValue {
    pub fn new() -> Self {
        Self { hidden: () }
    }
}

impl Value for NoneValue {
    fn test_value(&self) -> bool {
        false
    }

    /// Immutable
    fn freeze_value(&self) {}

    fn python_str(&self) -> String {
        "None".to_string()
    }
}

pub struct BoolValue {
    value: bool,
}

impl BoolValue {
    pub fn new(value: bool) -> Self {
        Self { value }
    }
}

impl Value for BoolValue {
    fn test_value(&self) -> bool {
        self.value
    }

    /// Immutable
    fn freeze_value(&self) {}

    fn python_str(&self) -> String {
        if self.value { "True" } else { "False" }.to_string()
    }
}

pub struct IntValue {
    value: i64,
}

impl IntValue {
    pub fn new(value: i64) -> Self {
        Self { value }
    }
}

impl Value for IntValue {
    fn test_value(&self) -> bool {
        self.value != 0
    }

    /// Immutable
    fn freeze_value(&self) {}

    fn python_str(&self) -> String {
        self.value.to_string()
    }
}

pub struct FloatValue {
    value: f64,
}

impl FloatValue {
    pub fn new(value: f64) -> Self {
        Self { value }
    }
}

impl Value for FloatValue {
    fn test_value(&self) -> bool {
        self.value != 0.
    }

    /// Immutable
    fn freeze_value(&self) {}

    fn python_str(&self) -> String {
        self.value.to_string()
    }
}

pub struct StringValue {
    value: String,
}

impl StringValue {
    pub fn new(value: String) -> Self {
        Self { value }
    }
}

impl Value for StringValue {
    fn test_value(&self) -> bool {
        !self.value.is_empty()
    }

    /// Immutable
    fn freeze_value(&self) {}

    fn python_str(&self) -> String {
        self.value.clone()
    }
}

pub struct TupleValue {
    elements: Vec<ObjectWeak<dyn Value>>,
}

impl TupleValue {
    pub fn new(elements: Vec<ObjectWeak<dyn Value>>) -> Self {
        Self { elements }
    }
}

impl Value for TupleValue {
    fn test_value(&self) -> bool {
        !self.elements.is_empty()
    }

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        out.extend_from_slice(&self.elements);
    }

    /// Immutable
    fn freeze_value(&self) {}

    fn python_str(&self) -> String {
        todo!()
    }
}

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
}

impl Value for ListValue {
    fn test_value(&self) -> bool {
        let state = self.state.lock().unwrap();
        !state.elements.is_empty()
    }

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        let mut state = self.state.lock().unwrap();
        out.extend_from_slice(&state.elements);
    }

    fn freeze_value(&self) {
        todo!()
    }

    fn python_str(&self) -> String {
        todo!()
    }
}

#[derive(Default)]
pub struct DictValue {
    state: Mutex<DictValueState>,
}

#[derive(Default)]
struct DictValueState {
    frozen: bool,
    num_iterators: usize,
    first_element: Option<usize>,
    last_element: Option<usize>,
    elements: Vec<DictValueElement>,
    // TODO: Support keys are arbitrary objects.
    keys_to_indices: HashMap<String, usize>,
}

struct DictValueElement {
    value: ObjectWeak<dyn Value>,
    next_index: Option<usize>,
    prev_index: Option<usize>,
}

impl DictValue {
    pub fn get(&self, key: &str) -> Result<Option<ObjectStrong<dyn Value>>> {
        let state = self.state.lock().unwrap();
        if let Some(index) = state.keys_to_indices.get(key) {
            let obj = state.elements[*index]
                .value
                .upgrade()
                .ok_or_else(|| err_msg("Dangling pointer in dict"))?;

            Ok(Some(obj))
        } else {
            Ok(None)
        }
    }

    pub fn insert(&self, key: &str, value: ObjectWeak<dyn Value>) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.check_can_mutate()?;

        if let Some(existing_index) = state.keys_to_indices.get(key).cloned() {
            state.elements[existing_index].value = value;
            return Ok(());
        }

        let index = state.elements.len();
        let element = DictValueElement {
            value,
            next_index: None,
            prev_index: state.last_element.clone(),
        };
        state.elements.push(element);
        state.keys_to_indices.insert(key.to_string(), index);

        if let Some(idx) = state.last_element {
            state.elements[idx].next_index = Some(index);
        } else {
            state.first_element = Some(index);
        }

        state.last_element = Some(index);

        Ok(())
    }

    pub fn remove(&self, key: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.check_can_mutate()?;

        let index = match state.keys_to_indices.remove(key) {
            Some(v) => v,
            None => return Ok(()),
        };

        // Step 1: Remove references to the index.
        {
            let prev_index = state.elements[index].prev_index.clone();
            let next_index = state.elements[index].next_index.clone();

            if let Some(idx) = prev_index {
                state.elements[idx].next_index = next_index;
            } else {
                state.first_element = next_index;
            }

            if let Some(idx) = next_index {
                state.elements[idx].prev_index = prev_index;
            } else {
                state.last_element = prev_index;
            }
        }

        // Step 2: Swap remove it and repair the new element which is at that position.
        state.elements.swap_remove(index);
        if index < state.elements.len() {
            let prev_index = state.elements[index].prev_index.clone();
            let next_index = state.elements[index].next_index.clone();

            if let Some(idx) = prev_index {
                state.elements[idx].next_index = Some(idx);
            } else {
                state.first_element = Some(index);
            }

            if let Some(idx) = next_index {
                state.elements[idx].prev_index = Some(idx)
            } else {
                state.last_element = Some(index);
            }
        }

        Ok(())
    }
}

impl Value for DictValue {
    fn test_value(&self) -> bool {
        let state = self.state.lock().unwrap();
        !state.elements.is_empty()
    }

    fn freeze_value(&self) {
        let mut state = self.state.lock().unwrap();
        state.frozen = true;
    }

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        let mut state = self.state.lock().unwrap();
        for el in &state.elements {
            out.push(el.value.clone());
        }
    }

    fn python_str(&self) -> String {
        todo!()
    }
}

impl DictValueState {
    fn check_can_mutate(&self) -> Result<()> {
        if self.frozen {
            return Err(err_msg("Can't mutate frozen dict"));
        }

        if self.num_iterators > 0 {
            return Err(err_msg("Can't mutate dict while iterating"));
        }

        Ok(())
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
