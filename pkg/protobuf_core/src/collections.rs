use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use common::const_default::ConstDefault;

use crate::reflection::*;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct MapField<K: Clone + PartialEq + Hash + Eq, V: Clone + PartialEq + Eq> {
    pub inner: Option<HashMap<K, V>>,
}

impl<K: Clone + PartialEq + Hash + Eq, V: Clone + PartialEq + Eq> ConstDefault for MapField<K, V> {
    const DEFAULT: Self = Self { inner: None };
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct SetField<T: PartialEq + Eq + Hash> {
    inner: Option<HashSet<T>>,
}

impl<T: Eq + Hash> ConstDefault for SetField<T> {
    const DEFAULT: Self = Self { inner: None };
}

impl<T: Eq + Hash> SetField<T> {
    fn get_mut(&mut self) -> &mut HashSet<T> {
        self.inner.get_or_insert_with(|| HashSet::new())
    }

    pub fn clear(&mut self) {
        self.get_mut().clear()
    }

    pub fn contains<Q: ?Sized>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Eq + Hash,
    {
        if let Some(set) = &self.inner {
            set.contains(value)
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        if let Some(set) = &self.inner {
            set.len()
        } else {
            0
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the set already contained the given value
    pub fn insert(&mut self, value: T) -> bool {
        self.get_mut().insert(value)
    }

    /// Returns whether or not the value was present before the removal.
    pub fn remove<Q: ?Sized>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Eq + Hash,
    {
        self.get_mut().remove(value)
    }

    pub fn iter(&self) -> SetFieldIter<T> {
        SetFieldIter {
            iter: self.inner.as_ref().map(|s| s.iter()),
        }
    }
}

pub struct SetFieldIter<'a, T> {
    iter: Option<std::collections::hash_set::Iter<'a, T>>,
}

impl<'a, T> Iterator for SetFieldIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(iter) = &mut self.iter {
            iter.next()
        } else {
            None
        }
    }
}

pub trait SetFieldReflectableElement = Reflect + Eq + Hash + Default + Clone;

impl<T: SetFieldReflectableElement> Reflect for SetField<T> {
    fn reflect(&self) -> Reflection {
        Reflection::Set(self)
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Set(self)
    }
}

impl<T: SetFieldReflectableElement> SetFieldReflection for SetField<T> {
    fn len(&self) -> usize {
        SetField::len(self)
    }

    fn entry<'a>(&'a self) -> Box<dyn SetFieldEntryReflection + 'a> {
        Box::new(SetFieldEntry {
            field: self,
            field_lifetime: PhantomData,
            value: T::default(),
        })
    }

    fn entry_mut<'a>(&'a mut self) -> Box<dyn SetFieldEntryReflectionMut + 'a> {
        Box::new(SetFieldEntry {
            field: self,
            field_lifetime: PhantomData,
            value: T::default(),
        })
    }

    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = Reflection<'a>> + 'a> {
        Box::new(SetField::iter(self).map(|v| v.reflect()))
    }
}

struct SetFieldEntry<'a, T: SetFieldReflectableElement, F: 'a + Deref<Target = SetField<T>>> {
    field: F,
    field_lifetime: PhantomData<&'a ()>,
    value: T,
}

impl<'a, T: SetFieldReflectableElement, F: 'a + Deref<Target = SetField<T>>> SetFieldEntryReflection
    for SetFieldEntry<'a, T, F>
{
    fn value(&mut self) -> ReflectionMut {
        self.value.reflect_mut()
    }

    fn contains(&self) -> bool {
        self.field.contains(&self.value)
    }
}

impl<'a, T: SetFieldReflectableElement, F: 'a + Deref<Target = SetField<T>> + DerefMut>
    SetFieldEntryReflectionMut for SetFieldEntry<'a, T, F>
{
    fn insert(&mut self) -> bool {
        self.field.insert(self.value.clone())
    }

    fn remove(&mut self) -> bool {
        self.field.remove(&self.value)
    }
}
