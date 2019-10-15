

pub trait Factory<T: ?Sized>: Send {
	fn create(&self) -> Box<T>;

	fn box_clone(&self) -> Box<dyn Factory<T>>;
}

// /// Allow any &factory to be based as a factory.
// impl<T, F: Factory<T>> Factory<T> for &F {
// 	fn create(&self) -> Box<T> {
// 		(*self).create()
// 	}
// }

// pub struct DefaultFactory<T: Default + ?Sized> {
// 	t: std::marker::PhantomData<T>
// }

// impl<T: Default + ?Sized> DefaultFactory<T> {
// 	pub fn new() -> Self {
// 		Self { t: std::marker::PhantomData }
// 	}
// }

// impl<T: Default + ?Sized + 'static> Factory<T> for DefaultFactory<T> {
// 	fn create(&self) -> Box<T> {
// 		Box::new(T::default())
// 	}
// }

// pub trait CloneDyn {
// 	fn clone_dyn(&self) -> Box<Self>;
// }

// impl<T: CloneDyn> Clone for Box<T> {
// 	fn clone(&self) -> Self {
// 		self.clone_dyn()
// 	}
// }