use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::hash::Hash;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use std::collections::{HashMap, HashSet};

use common::const_default::ConstDefault;

use crate::reflection::*;

// TODO: Move into a shared crate.
struct OptionIter<T> {
    iter: Option<T>
}

impl<T: Iterator> Iterator for OptionIter<T> {
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(iter) = &mut self.iter {
            iter.next()
        } else {
            None
        }
    }
}


#[derive(Default, Clone, Debug, PartialEq)]
pub struct MapField<K: Clone + PartialEq + Hash + Eq, V: Clone> {
    inner: Option<HashMap<K, V>>,
}

impl<K: Clone + PartialEq + Hash + Eq, V: Clone> ConstDefault for MapField<K, V> {
    const DEFAULT: Self = Self { inner: None };
}

impl<K: Clone + PartialOrd + Ord + PartialEq + Hash + Eq, V: Clone> MapField<K, V> {
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let map = self.inner.get_or_insert_with(|| HashMap::default());
        map.insert(key, value)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&K, &V)> {
        OptionIter {
            iter: self.inner.as_ref().map(|v| v.iter()),
        }
    }

    pub fn entries_sorted(&self) -> impl Iterator<Item = (&K, &V)> {
        let iter = self.inner.as_ref().map(|inner| {
            let mut keys = inner.keys().collect::<Vec<&K>>();
            keys.sort();

            keys.into_iter().map(move |k| (k, inner.get(k).unwrap()))
        });

        OptionIter { iter }
    }

    pub fn get<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.inner.as_ref().and_then(|map| map.get(k))
    }
}

impl<K: Clone + PartialEq + Hash + Eq, V: Clone> Reflect for MapField<K, V> {
    fn reflect(&self) -> Reflection {
        Reflection::Repeated(self)
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        ReflectionMut::Repeated(self)
    }
}

impl<K: Clone + PartialEq + Hash + Eq, V: Clone> RepeatedFieldReflection for MapField<K, V> {
    fn reflect_add(&mut self) -> ReflectionMut {
        todo!()
    }

    fn reflect_get(&self, index: usize) -> Option<Reflection> {
        todo!()
    }

    fn reflect_get_mut(&mut self, index: usize) -> Option<ReflectionMut> {
        todo!()
    }

    fn reflect_len(&self) -> usize {
        todo!()
    }
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

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        OptionIter {
            iter: self.inner.as_ref().map(|s| s.iter()),
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
