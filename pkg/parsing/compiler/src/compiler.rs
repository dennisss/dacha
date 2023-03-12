use common::errors::*;
use common::line_builder::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::buffer::BufferType;
use crate::enum_type::EnumType;
use crate::primitive::PrimitiveType;
use crate::proto::*;
use crate::size::SizeExpression;
use crate::struct_type::StructType;
use crate::types::*;

/// Compiles a BinaryDescriptorLibrary proto into Rust code that can be used to
/// interface with the listed binary types.
pub struct Compiler {}

/// Stores all types known to the compiler.
///
/// Each type is stored behind an Rc<> pointer where normally the only copy of
/// it exists in this struct. Types themselves may reference other types but
/// will only be given Weak<> pointers to ensure that cyclic references are
/// supported.
struct CompilerTypeIndex<'a> {
    /// Arena of structs and enums which can be referenced by name in other type
    /// declarations.
    ///
    /// This will include all the root types in the
    named_types: HashMap<&'a str, Rc<TypeCell<'a>>>,

    /// Arena of types with no explicit name (like primitives or buffers).
    /// This are referenced somewhere inside of the root types.
    anonymous_types: Vec<Rc<dyn TypePointer<'a> + 'a>>,
}

impl<'a> CompilerTypeIndex<'a> {
    fn new() -> Self {
        Self {
            anonymous_types: vec![],
            named_types: HashMap::new(),
        }
    }
}

impl<'a> TypeResolver<'a> for CompilerTypeIndex<'a> {
    fn resolve_type(
        &mut self,
        proto: &'a TypeProto,
        context: &TypeResolverContext,
    ) -> Result<TypeReference<'a>> {
        match proto.type_case() {
            TypeProtoTypeCase::Primitive(p) => {
                // TODO: Use cached copies when using the same type.

                let typ = Rc::new(PrimitiveType::create(p.clone(), context.endian))
                    as Rc<dyn TypePointer>;
                self.anonymous_types.push(typ.clone());
                Ok(TypeReference::new(Rc::downgrade(&typ)))
            }
            TypeProtoTypeCase::Buffer(buf) => {
                let typ = Rc::new(BufferType::create(buf, self, context)?) as Rc<dyn TypePointer>;
                self.anonymous_types.push(typ.clone());
                Ok(TypeReference::new(Rc::downgrade(&typ)))
            }
            TypeProtoTypeCase::Named(name) => {
                let typ = self
                    .named_types
                    .get(name.as_str())
                    .ok_or_else(|| format_err!("Unknown type named: {}", name))?
                    .clone() as Rc<dyn TypePointer>;

                Ok(TypeReference::new(Rc::downgrade(&typ)))
            }
            TypeProtoTypeCase::Unknown => Err(err_msg("Unspecified type")),
        }
    }
}

/*
    Next steps:
    - Need bit fields.
    - Make sure that Vec<u8> becomes Bytes and ideally parses from Bytes directly without copies.

    - Need a golden based regression test.
    - Need to support parsing into a refernce

    - TODO: In some cases, if we have a union field, we may want to just store it as bytes and then later if we need to, it can lookup values as needed.
*/

impl Compiler {
    pub fn compile(lib: &BinaryDescriptorLibrary) -> Result<String> {
        let mut lines = LineBuilder::new();
        lines.add("use ::alloc::vec::Vec;");
        lines.nl();
        lines.add("use ::common::errors::*;");
        lines.add("use ::parsing::parse_next;");
        lines.nl();

        let mut index = CompilerTypeIndex::new();

        let names = lib
            .structs()
            .iter()
            .map(|s| s.name())
            .chain(lib.enums().iter().map(|e| e.name()))
            .collect::<Vec<_>>();

        for name in names.iter().cloned() {
            let t = Rc::new(TypeCell::new());
            if !index.named_types.insert(name, t).is_none() {
                return Err(format_err!("Duplicate entity named: {}", name));
            }
        }

        for s in lib.structs() {
            let v = Box::new(StructType::create(s, &mut index)?);
            index.named_types[s.name()].set(v);
        }

        for e in lib.enums() {
            let v = Box::new(EnumType::create(e, &mut index)?);
            index.named_types[e.name()].set(v);
        }

        for name in names.iter().cloned() {
            index.named_types[name]
                .get_type()
                .compile_declaration(&mut lines)?;
        }

        Ok(lines.to_string())
    }
}
