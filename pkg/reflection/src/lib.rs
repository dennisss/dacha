pub trait Reflect {
    fn fields_index_mut(&mut self, index: usize) -> ReflectField;
    fn fields_len(&self) -> usize;
}

//impl Reflect {
//	fn fields_iter_mut(&mut self) -> ReflectFieldIterMut<Self> {
//		ReflectFieldIterMut { value: self, index: 0 }
//	}
//}
//
//pub struct ReflectFieldIterMut<'a, T: Reflect + ?Sized> {
//	value: &'a mut T,
//	index: usize
//}
//
//impl<'a, T: Reflect + ?Sized> Iterator for ReflectFieldIterMut<'a, T> {
//	type Item = ReflectField<'a>;
//
//	fn next(&mut self) -> Option<Self::Item> {
//		if self.index < self.value.fields_len() {
//			let field = self.field_index_mut(self.index);
//			self.index += 1;
//			Some(field)
//		} else {
//			None
//		}
//	}
//}

pub struct ReflectField<'a> {
    pub tags: &'static [&'static str],
    pub value: ReflectValue<'a>,
}

pub enum ReflectValue<'a> {
    String(&'a mut String),
    U64(&'a mut u64),
    I64(&'a mut i64),
    U32(&'a mut u32),
    I32(&'a mut i32),
    U16(&'a mut u16),
    I16(&'a mut i16),
    U8(&'a mut u8),
    U8Slice(&'a mut [u8]),
}

/*
    impl<T: Hello> Apples {}

    In a Token tree, look up all impl blocks
*/

//pub trait Reflect {
//
////	fn reflect(&mut self)
//}
