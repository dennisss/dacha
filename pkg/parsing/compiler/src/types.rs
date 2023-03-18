use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Deref;
use std::rc::{Rc, Weak};

use common::errors::*;
use common::line_builder::LineBuilder;
use common::EventuallyCell;

use crate::proto::*;
use crate::size::*;
use crate::struct_type::Field;

/// A type description which can be
///
/// NOTE: If any of these functions are called during Type
/// resolving/construction, they may panic if being accessed on a resolved type
/// which hasn't been initialized yet.
pub trait Type {
    fn compile_declaration(&self, out: &mut LineBuilder) -> Result<()> {
        Ok(())
    }

    fn type_expression(&self) -> Result<String>;

    fn default_value_expression(&self) -> Result<String> {
        Ok(format!("{}::default()", self.type_expression()?))
    }

    /// Constructs a Rust expression which evaluates to representation of
    /// 'value' as this type.
    fn value_expression(&self, value: &Value) -> Result<String> {
        Err(err_msg("Values not supported"))
    }

    /// Generates a string of code which evaluates to a parsed value of the type
    /// specified from an ambient buffer variable named 'input'. After the
    /// parsing is done, the code should also advance the 'input' buffer to
    /// the position after the value.
    ///
    /// TODO: For bit fields, this needs to be given a bit shift and mask to
    /// perform (only will work for primitives)
    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String>;

    fn parse_bits_expression(&self, bit_offset: usize, bit_width: usize) -> Result<String> {
        Err(err_msg("Can't parse type from bits"))
    }

    /// TODO: Pass in 'after_bytes' to this and use it.
    fn serialize_bytes_expression(
        &self,
        value: &str,
        context: &TypeParserContext,
    ) -> Result<String>;

    fn serialize_bits_expression(
        &self,
        value: &str,
        bit_offset: usize,
        bit_width: usize,
    ) -> Result<String> {
        Err(err_msg("Can't serialize type to bits"))
    }

    /// If statically known, then will get the length of the given type in
    /// bytes.
    ///
    /// NOTE: This won't return the correct value if this is the type of a field
    /// using This is used primarily
    ///
    /// TODO: Will need to know the name
    fn sizeof(&self, field_name: &str) -> Result<Option<SizeExpression>>;
}

pub trait TypePointer<'a> {
    fn get_type<'b>(&'b self) -> &'b (dyn Type + 'a);
}

impl<'a, T: Type + 'a> TypePointer<'a> for T {
    fn get_type(&self) -> &(dyn Type + 'a) {
        self
    }
}

pub struct TypeParserContext<'a, 'b> {
    /// Expression evaluating to the number of bytes that should follow the
    /// contents of the input buffer after the field is parsed (if it is
    /// well known at this point).
    ///
    /// This is used to determine where the end is for an end terminated field.
    pub after_bytes: Option<String>,

    // TODO: Remove this and only use the arguments.
    pub scope: &'a HashMap<&'b str, Field<'b>>,

    // TODO: Need to validate that the types fed in are compatible with the types
    pub arguments: &'a HashMap<&'b str, String>,
}

pub enum TypeSerializeValue<'a> {
    Constant(&'a Value),
    Expression(&'a str),
}

pub trait TypeResolver<'a> {
    fn resolve_type(
        &mut self,
        proto: &'a TypeProto,
        context: &TypeResolverContext,
    ) -> Result<TypeReference<'a>>;
}

pub struct TypeResolverContext {
    pub endian: Endian,
}

pub struct TypeReference<'a> {
    inner: Weak<dyn TypePointer<'a> + 'a>,
}

impl<'a> TypeReference<'a> {
    pub fn new(inner: Weak<dyn TypePointer<'a> + 'a>) -> Self {
        Self { inner }
    }

    pub fn get<'b>(&'b self) -> TypeHandle<'a, 'b> {
        TypeHandle {
            inner: self.inner.upgrade().unwrap(),
            lifetime: PhantomData,
        }
    }
}

// We do not allow direct access to the Rc<> unless pinned to a Weak<> pointer
// to avoid storing potentially cyclic references.
pub struct TypeHandle<'a, 'b> {
    inner: Rc<dyn TypePointer<'a> + 'a>,
    lifetime: PhantomData<&'b ()>,
}

impl<'a, 'b> Deref for TypeHandle<'a, 'b> {
    type Target = dyn Type + 'a;

    fn deref<'c>(&'c self) -> &'c Self::Target {
        self.inner.get_type()
    }
}

pub struct TypeCell<'a> {
    inner: EventuallyCell<Box<dyn Type + 'a>>,
}

impl<'a> TypeCell<'a> {
    pub fn new() -> Self {
        Self {
            inner: EventuallyCell::default(),
        }
    }

    pub fn set(&self, typ: Box<dyn Type + 'a>) {
        self.inner.set(typ);
    }
}

impl<'a> TypePointer<'a> for TypeCell<'a> {
    fn get_type<'b>(&'b self) -> &'b (dyn Type + 'a) {
        self.inner.get().as_ref()
    }
}
