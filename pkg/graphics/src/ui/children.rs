use core::any::TypeId;
use core::ops::{Deref, DerefMut};
use std::collections::HashMap;

use common::errors::*;

use crate::ui::element::Element;
use crate::ui::view::View;

pub type Key = (TypeId, String);

pub struct Children {
    values: Vec<Box<dyn View>>,
    keys: Vec<Key>,
}

impl Children {
    pub fn new(elements: &[Element]) -> Result<Self> {
        let mut inst = Self {
            values: vec![],
            keys: vec![],
        };

        inst.update(elements)?;
        Ok(inst)
    }

    pub fn update(&mut self, new_elements: &[Element]) -> Result<()> {
        // TODO: Improve the time complexity of this.

        let mut new_values = vec![];
        let mut new_keys = vec![];
        for el in new_elements {
            let key = {
                let (tid, key) = el.inner.key();
                (tid, key.to_string())
            };

            let mut existing_value = None;
            for i in 0..self.values.len() {
                if self.keys[i] == key {
                    existing_value = Some(self.values.remove(i));
                    self.keys.remove(i);
                    break;
                }
            }

            if let Some(mut value) = existing_value {
                value.update(el)?;
                new_values.push(value);
            } else {
                new_values.push(el.inner.instantiate()?);
            }

            new_keys.push(key);
        }

        self.values = new_values;
        self.keys = new_keys;

        Ok(())
    }
}

impl Deref for Children {
    type Target = [Box<dyn View>];

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl DerefMut for Children {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}
