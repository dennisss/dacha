use common::errors::*;
use common::line_builder::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::buffer::BufferType;
use crate::enum_type::EnumType;
use crate::primitive::PrimitiveType;
use crate::proto::*;
use crate::string::StringType;
use crate::struct_type::StructType;
use crate::types::*;
use crate::union_type::UnionType;

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

    fn wrap_type<'b, T: Type + 'b>(typ: T) -> (Rc<dyn TypePointer<'b> + 'b>, TypeReference<'b>) {
        let typ = Rc::new(typ) as Rc<dyn TypePointer>;

        let ptr = typ.clone();
        let refernce = TypeReference::new(Rc::downgrade(&typ));
        (ptr, refernce)
    }

    fn add_anonymous_type<T: Type + 'a>(&mut self, typ: T) -> TypeReference<'a> {
        let (ptr, reference) = Self::wrap_type(typ);

        self.anonymous_types.push(ptr);
        reference
    }
}

impl<'a> TypeResolver<'a> for CompilerTypeIndex<'a> {
    fn resolve_type(
        &mut self,
        proto: &'a TypeProto,
        context: &TypeResolverContext,
    ) -> Result<TypeReference<'a>> {
        Ok(match proto.typ_case() {
            TypeProtoTypeCase::Primitive(p) => {
                // TODO: Use cached copies when using the same type.

                self.add_anonymous_type(PrimitiveType::create(p.clone(), context.endian))
            }
            TypeProtoTypeCase::Buffer(buf) => {
                let typ = BufferType::create(buf.as_ref(), self, context)?;
                self.add_anonymous_type(typ)
            }
            TypeProtoTypeCase::Named(name) => {
                let typ = self
                    .named_types
                    .get(name.as_str())
                    .ok_or_else(|| format_err!("Unknown type named: {}", name))?
                    .clone() as Rc<dyn TypePointer>;

                TypeReference::new(Rc::downgrade(&typ))
            }
            TypeProtoTypeCase::String(s) => {
                let typ = StringType::create(s.as_ref(), self, context)?;
                self.add_anonymous_type(typ)
            }
            TypeProtoTypeCase::NOT_SET => return Err(err_msg("Unspecified type")),
        })
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
    pub fn compile(mut lib: BinaryDescriptorLibrary) -> Result<String> {
        Self::rewrite_length_field_values(&mut lib);

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
            .chain(lib.unions().iter().map(|u| u.name()))
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

        for u in lib.unions() {
            let v = Box::new(UnionType::create(u, &mut index)?);
            index.named_types[u.name()].set(v);
        }

        for name in names.iter().cloned() {
            index.named_types[name]
                .get_type()
                .compile_declaration(&mut lines)?;
        }

        Ok(lines.to_string())
    }

    fn rewrite_length_field_values(lib: &mut BinaryDescriptorLibrary) {
        for s in lib.structs_mut() {
            Self::rewrite_length_field_values_in_struct(s);
        }
    }

    fn rewrite_length_field_values_in_struct(proto: &mut Struct) {
        let mut new_values = HashMap::new();

        for field in proto.field_mut() {
            if let TypeProtoTypeCase::Buffer(buf) = field.typ_mut().typ_case_mut() {
                if let BufferTypeProtoSizeCase::LengthFieldName(name) = buf.size_case() {
                    let name = name.clone();

                    buf.set_length(name.clone());

                    // If the buffer's presence isn't a subset of the length field's presence, then
                    // we can't always derive the value of the length field.
                    // TODO: Actually check for proper subsets.
                    if field.presence().is_empty() {
                        new_values.insert(name.clone(), format!("{}.len()", field.name()));
                    }

                    let mut arg = FieldArgument::default();
                    arg.set_name(&name);
                    arg.set_value(&name);
                    field.add_argument(arg);
                }
            }
        }

        for field in proto.field_mut() {
            if !field.value().is_empty() {
                continue;
            }

            if let Some(value) = new_values.get(field.name()) {
                field.set_value(value);
            }
        }
    }
}
