use core::cmp::{Eq, PartialEq};
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::ops::{Add, Deref, DerefMut, Index, IndexMut};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EntityStorage<IdT, T> {
    values: HashMap<Id<IdT>, T>,
    pub(super) next_id: Id<IdT>,
}

impl<IdT: Clone + Copy, T> EntityStorage<IdT, T> {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            next_id: Id::zero(),
        }
    }

    pub fn unique_id(&mut self) -> Id<IdT> {
        let id = self.next_id;
        self.next_id = self.next_id + 1;
        id
    }
}

impl<IdT: Hash + PartialEq + Eq, T> Index<Id<IdT>> for EntityStorage<IdT, T> {
    type Output = T;

    fn index(&self, index: Id<IdT>) -> &Self::Output {
        &self.values[&index]
    }
}

impl<IdT: Hash + PartialEq + Eq, T> IndexMut<Id<IdT>> for EntityStorage<IdT, T> {
    fn index_mut(&mut self, index: Id<IdT>) -> &mut Self::Output {
        self.values.get_mut(&index).unwrap()
    }
}

impl<IdT, T> Deref for EntityStorage<IdT, T> {
    type Target = HashMap<Id<IdT>, T>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl<IdT, T> DerefMut for EntityStorage<IdT, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct FaceTag {
    hidden: (),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct EdgeTag {
    hidden: (),
}

pub type FaceId = Id<FaceTag>;
pub type EdgeId = Id<EdgeTag>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Hash)]
pub struct Id<T>(usize, PhantomData<T>);

impl<T> Id<T> {
    pub fn zero() -> Self {
        Self(0, PhantomData)
    }
}

impl<T> Add for Id<T> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0, PhantomData)
    }
}

impl<T> Add<usize> for Id<T> {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self(self.0 + rhs, PhantomData)
    }
}
