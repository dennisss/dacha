use std::collections::LinkedList;
/// A HashMap which allows multiple values to be inserted at the same key
/// But, we will only distinct values for a single key.
pub struct MultiHashMap<K, V> {
	map: std::collections::HashMap<K, LinkedList<V>>
}

impl<K: std::hash::Hash + Eq, V: Eq> MultiHashMap<K, V> {

	pub fn new() -> Self {
		MultiHashMap { map: std::collections::HashMap::new() }
	}

	// TODO: Ideally we will enforce a max limit on number of items per key for deflate
	/// Inserts a key value pair into the map.
	/// If the value is already present at the key, then nothing will change.
	///
	/// Returns whether or not the value was newly inserted.
	pub fn insert(&mut self, k: K, v: V) -> bool {
		if let Some(list) = self.map.get_mut(&k) {
			for i in list.iter() {
				if i == &v {
					return false;
				}
			}

			list.push(v);
		} else {
			let list = LinkedList::new();
			list.push_back(v);
			self.map.insert(k, list);
		}

		true
	}

	/// Remove the key value pair from the map.
	/// 
	/// Returns the old value that was removed. Or none if it didn't exist.
	pub fn remove(&mut self, k: &K, v: &V) -> Option<V> {
		let mut val = None; 
		if let Some(list) = self.map.get_mut(k) {
			for i in 0..list.len() {
				if &list[i] == v {
					val = Some(list.remove(i));
				}
			}

			if list.len() == 0 {
				self.map.remove(k);
			}
		}

		val
	}

	/// Gets a mutable reference to the list of all values associated with a key.
	/// TODO: Hide the implementation details of the list?
	pub fn get_key_mut(&mut self, k: &K) -> Option<&mut LinkedList<V>> {
		self.map.get_mut(k)
	}

	pub fn get_key(&self, k: &K) -> Option<&[V]> {
		self.map.get(k).map(|v| v.as_ref())
	}

}
