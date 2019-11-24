use std::sync::Arc;
use std::ops::{Deref, DerefMut};
use std::cell::RefCell;
use crate::comparator::*;
use std::borrow::Borrow;

static _DUMMY_COMPARATOR: DummyComparator = DummyComparator::new();

thread_local! {
	static CURRENT_COMPARATOR: RefCell<&'static dyn Comparator> =
		RefCell::new(&_DUMMY_COMPARATOR);
}

/// A wrapper around a type which can only accessor with a well defined
/// comparator.
pub struct ComparatorContext<T> {
	inner: T,
	comparator: Arc<dyn Comparator>
}

impl<T> ComparatorContext<T> {
	pub fn new(inner: T, comparator: Arc<dyn Comparator>) -> Self {
		Self { inner, comparator }
	}

	/// NOTE: This pointer should not be saved.
	pub fn comparator() -> &'static dyn Comparator {
		CURRENT_COMPARATOR.with(|v| { *v.borrow() })
	}

	fn set_comparator(&self) {
		CURRENT_COMPARATOR.with(|v| {
			v.replace(unsafe { std::mem::transmute(self.comparator.as_ref()) });
		});
	}

	fn unset_comparator() {
		CURRENT_COMPARATOR.with(|v| {
			v.replace(&_DUMMY_COMPARATOR);
		});
	}

	pub fn guard(&self) -> ComparatorContextGuard<T> {
		self.set_comparator();
		ComparatorContextGuard { inner: &self.inner }
	}

	pub fn guard_mut(&mut self) -> ComparatorContextGuardMut<T> {
		self.set_comparator();
		ComparatorContextGuardMut { inner: &mut self.inner }
	}
}

pub struct ComparatorContextGuard<'a, T> {
	inner: &'a T
}

impl<'a, T> ComparatorContextGuard<'a, T> {
	/// Similar to deref() except associates ownership with the
	/// ComparatorContext instead of with the guard.
	pub fn inner(&self) -> &'a T { self.inner }
}

impl<T> Drop for ComparatorContextGuard<'_, T> {
	fn drop(&mut self) {
		// NOTE: This implies that multiple immutable guards can't be checked
		// out at once.
		ComparatorContext::<T>::unset_comparator();
	}
}

impl<T> Deref for ComparatorContextGuard<'_, T> {
	type Target = T;
	fn deref(&self) -> &Self::Target { self.inner }
}

pub struct ComparatorContextGuardMut<'a, T> {
	inner: &'a mut T
}

impl<T> Drop for ComparatorContextGuardMut<'_, T> {
	fn drop(&mut self) {
		ComparatorContext::<T>::unset_comparator();
	}
}

impl<T> Deref for ComparatorContextGuardMut<'_, T> {
	type Target = T;
	fn deref(&self) -> &Self::Target { self.inner }
}

impl<T> DerefMut for ComparatorContextGuardMut<'_, T> {
	fn deref_mut(&mut self) -> &mut Self::Target { self.inner }
}