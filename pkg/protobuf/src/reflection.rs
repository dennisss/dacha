
pub type FieldNumber = usize;

pub enum Reflection<'a> {
	F32(PrimitiveReflection<'a, f32>),
	F64(PrimitiveReflection<'a, f64>),
	I32(PrimitiveReflection<'a, i32>),
	I64(PrimitiveReflection<'a, i64>),
	U32(PrimitiveReflection<'a, u32>),
	U64(PrimitiveReflection<'a, u64>),
	Bool(PrimitiveReflection<'a, bool>),
	String,
	Bytes,
	Message(&'a dyn MessageReflection),
	Enum
}

pub enum ReflectionMut<'a> {
	F32(PrimitiveReflection<'a, f32>),
	F64(PrimitiveReflection<'a, f64>),
	I32(PrimitiveReflection<'a, i32>),
	I64(PrimitiveReflection<'a, i64>),
	U32(PrimitiveReflection<'a, u32>),
	U64(PrimitiveReflection<'a, u64>),
	Bool(PrimitiveReflection<'a, bool>),
	String,
	Bytes,
	Message(&'a mut dyn MessageReflection),
	Enum
}

pub struct PrimitiveReflection<'a, T> {
	pub value: &'a mut T
}

/// NOTE: Should be implemented by all Messages.
pub trait MessageReflection {

	// A non-mutable version would be required for the regular

	// Should also have a fields() which iterates over fields?


	// Some fields may also have an empty name to indicate that they are unknown
//	fn fields(&self) -> &'static [(&FieldNumber)];

	fn field_by_number(&self, num: FieldNumber) -> Option<Reflection>;

	fn field_by_number_mut(&mut self, num: FieldNumber) -> Option<ReflectionMut>;

	fn field_number_by_name(&self, name: &str) -> Option<FieldNumber>;

//	fn field_by_name_mut(&mut self, name: &str) -> Option<Reflection>;
}

// Next step would be?

pub trait Reflect {
	fn reflect(&mut self) -> Reflection;
}

impl Reflect for f32 {
	fn reflect(&mut self) -> Reflection {
		Reflection::F32(PrimitiveReflection { value: self })
	}
}
