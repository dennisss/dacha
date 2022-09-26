#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

pub struct VecHashSet<K, V> {
    keys: Vec<K>,
    values: Vec<V>,
    indices: HashMap<K, usize>,
}

impl<K: Hash + Eq + Clone, V> VecHashSet<K, V> {
    pub fn new() -> Self {
        Self {
            keys: vec![],
            values: vec![],
            indices: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: K, mut value: V) -> Option<V> {
        if let Some(index) = self.indices.get(&key) {
            std::mem::swap(&mut value, &mut self.values[*index]);
            Some(value)
        } else {
            self.indices.insert(key.clone(), self.values.len());
            self.keys.push(key);
            self.values.push(value);
            None
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(index) = self.indices.remove(key) {
            self.keys.swap_remove(index);
            let value = self.values.swap_remove(index);
            // Fix the index of the swapped entry.
            if index < self.values.len() {
                self.indices.insert(self.keys[index].clone(), index);
            }

            Some(value)
        } else {
            None
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.indices.contains_key(key)
    }

    pub fn keys(&self) -> &[K] {
        &self.keys
    }

    pub fn values(&self) -> &[V] {
        &self.values
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        if let Some(idx) = self.indices.get(key) {
            Some(&self.values[*idx])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        if let Some(idx) = self.indices.get(key) {
            Some(&mut self.values[*idx])
        } else {
            None
        }
    }
}
