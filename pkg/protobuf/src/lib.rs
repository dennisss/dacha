#![feature(proc_macro_hygiene, decl_macro, type_alias_enum_variants, generators, trait_alias, core_intrinsics)]

#[macro_use] extern crate common;
#[macro_use] extern crate parsing;
extern crate math;
extern crate byteorder;  // < Mainly needed for f32/f64 conversions
extern crate bytes;

pub use bytes::{BytesMut, Bytes};

use common::errors::*;

// NOTE: Construct an empty proto by calling MessageType::default()
pub trait Message: Clone + std::fmt::Debug + std::default::Default {
	// NOTE: This will append values to
	fn parse(data: Bytes) -> Result<Self>;

	/// Serializes the protobuf as a
	fn serialize(&self) -> Result<Vec<u8>>;

	// TODO: should be a shared reference?
	// fn descriptor() -> Descriptor;

//	fn parse_text(data: &str) -> Result<Self>;

	// TODO: Must also be able to parse a text proto

	// TODO: Serializers must return a result because required conditions may
	// not be satisfied.

//	fn debug_string(&self) -> String;

//	fn merge_from(&mut self, other: &Self);

	// fn unknown_fields() -> &[UnknownField];
}

/// Common trait implemented by all code generated protobuf enum types.
pub trait Enum: Copy {
	/// Should convert a number to a valid branch of the enum, or else should
	/// error out it the value is not in the enum.
	fn from_usize(v: usize) -> Result<Self>;

	fn to_usize(&self) -> usize;
}

pub mod spec;
pub mod tokenizer;
pub mod syntax;
pub mod wire;
pub mod compiler;
pub mod service;
pub mod reflection;
pub mod text;