
pub enum ScalarTypeSpec {
	Double,
	Float,
	Int32,
	Int64,
	Uint32,
	Uint64,
	Sint32,
	Sint64,
	Fixed32,
	Fixed64,
	SFixed32,
	SFixed64,
	Bool,
	Str,
	Bytes,
}

pub enum TypeSpec {
	Scalar(ScalarTypeSpec),


}

pub struct FieldSpec {
	pub name: String,
	pub num: u64,
	pub typ: TypeSpec,
	pub optional: bool,
	// TODO: Default value
}

pub struct MessageSpec {
	pub fields: Vec<FieldSpec>
}