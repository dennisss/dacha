use core::{borrow::Borrow, fmt::Display};
use std::collections::HashMap;
use std::hash::Hash;

use crate::Result;

pub trait HashMapExt<Q: ?Sized, V> {
    fn get_or_err(&self, key: &Q) -> Result<&V>;
}

impl<Q: ?Sized + Hash + Eq + Display, K: Eq + Hash + Borrow<Q>, V> HashMapExt<Q, V>
    for HashMap<K, V>
{
    fn get_or_err(&self, key: &Q) -> Result<&V> {
        self.get(key)
            .ok_or_else(|| format_err!("Missing key: {}", key))
    }
}
