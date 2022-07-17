use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::MutexGuard;

use common::errors::*;
use crypto::hasher::Hasher;

use crate::object::*;
use crate::value::*;
use crate::value_attributes;

/// TODO: Move of the logic for this into a generic OrderedHashMap class.
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

    /// Mapping from a key's hash to the indices of all elements with that hash.
    /// TODO: Optimize the key to inline the memory if there is only one element
    /// present.
    hash_to_indices: HashMap<u64, Vec<usize>>,
}

struct DictValueKey {
    hash: u64,
    key: ObjectWeak<dyn Value>,
}

struct DictValueElement {
    key_hash: u64,
    key: ObjectWeak<dyn Value>,
    value: ObjectWeak<dyn Value>,
    next_index: Option<usize>,
    prev_index: Option<usize>,
}

struct DictValueEntry {
    key_hash: u64,
    index: Option<usize>,
    missing_bucket: bool,
}

impl DictValue {
    /// TODO: Rename call_getitem
    pub fn get(
        &self,
        key: &dyn Value,
        frame: &mut ValueCallFrame,
    ) -> Result<Option<ObjectStrong<dyn Value>>> {
        let state = self.state.lock().unwrap();

        if let Some(index) = state.entry(key, frame)?.index {
            let obj = state.elements[index].value.upgrade_or_error()?;
            Ok(Some(obj))
        } else {
            Ok(None)
        }
    }

    /// TODO: rename call_setitem
    pub fn insert(
        &self,
        key: &ObjectStrong<dyn Value>,
        mut value: ObjectWeak<dyn Value>,
        frame: &mut ValueCallFrame,
    ) -> Result<Option<ObjectWeak<dyn Value>>> {
        let mut state = self.state.lock().unwrap();
        state.check_can_mutate()?;

        let entry = state.entry(&**key, frame)?;

        // The key already exists, just replace the value.
        if let Some(index) = entry.index {
            core::mem::swap(&mut state.elements[index].value, &mut value);
            return Ok(Some(value));
        }

        // Otherwise we need to insert it at the end.
        let index = state.elements.len();
        let element = DictValueElement {
            key: key.downgrade(),
            key_hash: entry.key_hash,
            value,
            next_index: None,
            prev_index: state.last_element.clone(),
        };
        state.elements.push(element);

        state
            .hash_to_indices
            .entry(entry.key_hash)
            .or_insert_with(|| vec![])
            .push(index);

        if let Some(idx) = state.last_element {
            state.elements[idx].next_index = Some(index);
        } else {
            state.first_element = Some(index);
        }

        state.last_element = Some(index);

        Ok(None)
    }

    pub fn remove(&self, key: &dyn Value, context: &mut ValueCallFrame) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.check_can_mutate()?;

        let entry = state.entry(key, context)?;

        let index = match entry.index {
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

            let indices = state.hash_to_indices.get_mut(&entry.key_hash).unwrap();
            let mut found = false;
            for i in 0..indices.len() {
                if indices[i] == index {
                    indices.swap_remove(i);
                    found = true;
                    break;
                }
            }
            assert!(found);
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

            let old_index = state.elements.len();
            let old_hash = state.elements[index].key_hash;
            let indices = state.hash_to_indices.get_mut(&old_hash).unwrap();
            let mut found = false;
            for i in 0..indices.len() {
                if indices[i] == old_index {
                    indices[i] = index;
                    found = true;
                    break;
                }
            }
            assert!(found);
        }

        Ok(())
    }

    // TODO: Need a robust strategy to make sure this doesn't deadlock.
    pub fn iter<'a>(&'a self) -> DictValueExlusiveIterator<'a> {
        let state = self.state.lock().unwrap();
        let index = state.first_element;
        DictValueExlusiveIterator { state, index }
    }
}

impl Value for DictValue {
    value_attributes!(Mutable | ReprAsStr);

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        let mut state = self.state.lock().unwrap();
        for el in &state.elements {
            out.push(el.key.clone());
            out.push(el.value.clone());
        }
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
        todo!()
    }

    fn call_eq(&self, other: &dyn Value, context: &mut ValueCallFrame) -> Result<bool> {
        todo!()
    }

    fn call_len(&self, frame: &mut ValueCallFrame) -> Result<usize> {
        let state = self.state.lock().unwrap();
        Ok(state.elements.len())
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

    fn entry(&self, key: &dyn Value, context: &mut ValueCallFrame) -> Result<DictValueEntry> {
        let mut context = context.child(key)?;

        let key_hash = {
            let mut hasher = context.new_hasher();
            key.call_hash(&mut hasher, &mut context)?;
            hasher.finish_u64()
        };

        let bucket_indices = match self.hash_to_indices.get(&key_hash) {
            Some(v) => v,
            None => {
                return Ok(DictValueEntry {
                    key_hash,
                    index: None,
                    missing_bucket: true,
                })
            }
        };

        for index in bucket_indices.iter().cloned() {
            let el = &self.elements[index];
            assert_eq!(el.key_hash, key_hash);

            let cur_key = el.key.upgrade_or_error()?;

            if key.call_eq(&*cur_key, &mut context)? {
                return Ok(DictValueEntry {
                    index: Some(index),
                    key_hash,
                    missing_bucket: false,
                });
            }
        }

        Ok(DictValueEntry {
            key_hash,
            index: None,
            missing_bucket: false,
        })
    }
}

pub struct DictValueExlusiveIterator<'a> {
    state: MutexGuard<'a, DictValueState>,
    index: Option<usize>,
}

impl<'a> Iterator for DictValueExlusiveIterator<'a> {
    type Item = (ObjectWeak<dyn Value>, ObjectWeak<dyn Value>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(index) = self.index {
            let el = &self.state.elements[index];
            self.index = el.next_index;
            Some((el.key.clone(), el.value.clone()))
        } else {
            None
        }
    }
}

/// Controls which values are returned by DictIteratorValue::python_next().
pub enum DictIteratorMode {
    Keys,
    Items,
    Values,
}

pub struct DictIteratorValue {
    inst: ObjectWeak<dyn Value>,
    next_index: Mutex<Option<usize>>,
}

impl DictIteratorValue {
    // pub fn next(&self)
}
