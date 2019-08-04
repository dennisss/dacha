#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators, trait_alias, core_intrinsics)]

#[macro_use] extern crate nom;
#[macro_use] extern crate error_chain;

extern crate math;
extern crate byteorder; // < Mainly needed for f32/f64 conversions
extern crate num_traits;
extern crate bytes;

pub use bytes::{BytesMut, Bytes};

pub type Result<T> = std::result::Result<T, &'static str>;

// NOTE: Construct an empty proto by calling MessageType::default()
pub trait Message: Clone + std::fmt::Debug + std::default::Default {
	// TODO: should be a shared reference?
	// fn descriptor() -> Descriptor;

	// TODO: These parsers should return a result
	fn parse(data: Bytes) -> Result<Self>;

	fn parse_text(data: &str) -> Result<Self>;

	// TODO: Must also be able to parse a text proto

	// TODO: Serializers must return a result because required conditions may not be satisfied.

	/// Serializes the protobuf as a 
	fn serialize(&self) -> BytesMut;

	fn debug_string(&self) -> String;

	fn merge_from(&mut self, other: &Self);

	// fn unknown_fields() -> &[UnknownField];
}

/// Common trait implemented by all code generated protobuf enum types.
pub trait Enum: Copy {
	/// Should convert a number to a valid branch of the enum, or else should error out it the value is not in the enum.
	fn from_usize(v: usize) -> Result<Self>;
}

pub mod spec;
pub mod tokenizer;
pub mod syntax;
pub mod wire;
pub mod compiler;