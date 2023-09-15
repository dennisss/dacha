// Code for taking a parsed .proto file descriptor and performing code
// generation into Rust code.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;
use std::ops::Deref;

use common::errors::*;
use common::line_builder::*;
use crypto::hasher::Hasher;
use file::LocalPath;
use file::LocalPathBuf;
use protobuf_compiler_proto::dacha::*;
use protobuf_core::extension::ExtensionTag;
use protobuf_core::tokenizer::parse_str_lit_inner;
use protobuf_core::tokenizer::serialize_str_lit;
use protobuf_core::FieldNumber;
use protobuf_core::Message;
use protobuf_descriptor::EnumValueDescriptorProto;
use protobuf_descriptor::FieldDescriptorProto_Label;
use protobuf_descriptor::FieldDescriptorProto_Type;
use protobuf_dynamic::DescriptorPoolOptions;
use protobuf_dynamic::ExtendDescriptor;
use protobuf_dynamic::FieldDescriptor;
use protobuf_dynamic::OneOfDescriptor;
use protobuf_dynamic::TypeDescriptor;
use protobuf_dynamic::{
    DescriptorPool, EnumDescriptor, FileDescriptor, MessageDescriptor, ServiceDescriptor, Syntax,
};

use crate::escape::*;

// TODO: Lets not forget to serialize and parse unknown fields as well.

/*
    Operations to support:
    - Non-repeated fields:
        - has_name() -> bool

    - Repeated fields:
        - name() -> &[T]
        - name_mut() -> &mut [T]
        - add_name([v: T]) -> &mut T
        - name_len()
        - clear_name()

    - Primitive fields:
        .set_name(v: T) -> ()
        .clear_name() -> ()
*/
/*
    It would generally be more efficient to maintain a bitvector containing
*/

// google.protobuf.Descriptor.InnerMessage

// Given a field, support getting a statement for it's default

/*
    Query for Z in x.y.J
    - Check for .x.y.J.Z
    - Check for .x.y.Z
    - Check for .x.Z
    - Check for
*/

regexp!(CARGO_PACKAGE_NAME => "\\[package\\]\nname = \"([^\"]+)\"");

#[derive(Clone)]
pub struct CompilerOptions {
    /// Options used to initialize the descriptor pool.
    /// (used by protobuf_compiler::build())
    pub descriptor_pool_options: DescriptorPoolOptions,

    /// In not none, protos must be in these directories to be compiled.
    /// (used by protobuf_compiler::build())
    pub allowlisted_paths: Option<Vec<LocalPathBuf>>,

    /// Rust package/crate name of the 'protobuf' crate.
    /// Defaults to '::protobuf' and normally doesn't need to be changed.
    pub runtime_package: String,

    /// Rust package/crate name that contains the RPC implemention (for
    /// generated service definitions).
    pub rpc_package: String,

    /// Whether or not the generated code should be prettified with rustfmt.
    pub should_format: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            descriptor_pool_options: DescriptorPoolOptions::default(),
            runtime_package: "::protobuf".into(),
            rpc_package: "::rpc".into(),
            should_format: false,
            allowlisted_paths: None,
        }
    }
}

struct ResolvedType {
    /// Name of the type in the currently being compiled source file.
    typename: String,
    descriptor: TypeDescriptor,
}

struct ImportedProto {
    proto: FileDescriptor,
    package_path: String,
    file_id: String,
}

pub struct Compiler {
    // The current top level code string that we are building.
    outer: String,

    // Top level proto file descriptor that is being compiled
    file: FileDescriptor,

    imported_protos: HashMap<u32, ImportedProto>,

    options: CompilerOptions,

    file_id: String,
}

/*
    TODO:
    Things to validate about a proto file
    - All definitions at the same level have distinct names
    - Enum fields and message fields have have distinct names
    - All message fields have distinct numbers
*/

fn rust_bytestring(v: &[u8]) -> String {
    let mut out = String::new();
    out.reserve_exact(v.len() * 4 + 3);
    out.push_str("b\"");
    for b in v.iter() {
        if b.is_ascii_alphanumeric() {
            out.push(*b as char);
        } else {
            out.push_str(&format!("\\x{:02x}", *b));
        }
    }

    out.push('"');
    out
}

struct CompiledOneOf {
    typename: String,
    source: String,
}

struct MapField {
    field: FieldDescriptor,
    key_field: FieldDescriptor,
    value_field: FieldDescriptor,
}

impl Compiler {
    pub fn compile(
        file: FileDescriptor,
        current_package_dir: &LocalPath,
        options: &CompilerOptions,
    ) -> Result<(String, String)> {
        let mut c = Compiler {
            outer: String::new(),
            file: file.clone(),
            options: options.clone(),
            imported_protos: HashMap::new(),
            file_id: String::new(),
        };

        let file_id = {
            let id = crypto::sip::SipHasher::default_rounds_with_key_halves(0, 0)
                .finish_with(file.name().as_bytes());
            base_radix::hex_encode(&id).to_ascii_uppercase()
        };
        c.file_id = file_id;

        c.outer += "// AUTOGENERATED BY THE PROTOBUF COMPILER\n\n";

        let mut lines = LineBuilder::new();
        lines.add("#[cfg(feature = \"std\")] use std::sync::Arc;");
        lines.add("#[cfg(feature = \"alloc\")] use alloc::vec::Vec;");
        lines.add("#[cfg(feature = \"alloc\")] use alloc::string::String;\n");
        lines.add("#[cfg(feature = \"alloc\")] use alloc::boxed::Box;\n");
        lines.add("use common::errors::*;");
        lines.add("use common::list::Appendable;");
        lines.add("use common::collections::FixedString;");
        lines.add("use common::fixed::vec::FixedVec;");
        lines.add("use common::const_default::ConstDefault;");
        lines.add(format!("use {}::*;\n", options.runtime_package));
        lines.add(format!("use {}::codecs::*;\n", options.runtime_package));
        lines.add(format!("use {}::wire::*;\n", options.runtime_package));
        // lines.add(format!("use {}::service::*;\n", c.options.runtime_package));
        lines.add(format!(
            "#[cfg(feature = \"alloc\")] use {}::reflection::*;\n",
            options.runtime_package
        ));
        c.outer.push_str(&lines.to_string());

        // TODO: Have an in-process cache for reading imported descriptors from disk.
        for import_name in c.file.proto().dependency() {
            // Search for the crate in which this .proto file exists.
            // TODO: Eventually this can be information that is communicated via the build
            // system

            let import_file = file
                .pool()
                .find_file(&import_name)
                .ok_or_else(|| err_msg("Missing imported file"))?;

            let (mut rust_package_name, rust_package_dir) = {
                let mut rust_package = None;

                let mut current_dir = import_file.local_path().unwrap().parent();
                while let Some(dir) = current_dir.take() {
                    let toml_path = dir.join("Cargo.toml");
                    if !std::path::Path::new(toml_path.as_str()).try_exists()? {
                        // TODO: Don't search outside of the source directory.
                        current_dir = dir.parent();
                        continue;
                    }

                    let toml = std::fs::read_to_string(toml_path)?;

                    let m = CARGO_PACKAGE_NAME
                        .exec(&toml)
                        .ok_or_else(|| err_msg("Failed to get package name"))?;
                    let package_name = m.group_str(1).unwrap()?;

                    rust_package = Some((package_name.to_string(), dir));

                    break;
                }

                rust_package.ok_or_else(|| err_msg("Failed to find rust package"))?
            };

            // TODO: "/hello/" != "/hello"
            // assert_eq!(rust_package_dir, current_package_dir);
            if rust_package_dir.normalized() == current_package_dir.normalized() {
                rust_package_name = "crate".to_string();
            }

            let mut package_path = rust_package_name.to_string();
            for part in import_file.proto().package().split(".") {
                package_path.push_str("::");
                package_path.push_str(escape_rust_identifier(part));
            }

            // TODO: Dedup this code.
            let file_id = {
                let id = crypto::sip::SipHasher::default_rounds_with_key_halves(0, 0)
                    .finish_with(import_file.name().as_bytes());
                base_radix::hex_encode(&id).to_ascii_uppercase()
            };

            c.imported_protos.insert(
                import_file.index(),
                ImportedProto {
                    proto: import_file,
                    package_path,
                    file_id,
                },
            );
        }

        c.outer.push_str("\n");

        // Add the file descriptor
        {
            let proto = {
                let data = file.to_proto().serialize()?;
                rust_bytestring(&data)
            };

            let mut deps = vec![];
            for import in c.imported_protos.values() {
                deps.push(format!(
                    "// {}\n&{}::FILE_DESCRIPTOR_{},\n",
                    import.proto.name(),
                    import.package_path,
                    import.file_id
                ));
            }

            // TODO: Put all of these in a single ELF section which is away from everything
            // else as these are infrequently accessed,
            //
            // NOTE: This is public as
            // it will be referenced by other FILE_DESCRIPTORs in
            // other generated files.
            c.outer.push_str(&format!(
                "
            pub static FILE_DESCRIPTOR_{file_id}: {runtime_pkg}::StaticFileDescriptor = {runtime_pkg}::StaticFileDescriptor {{
                proto: {proto},
                dependencies: &[{deps}]
            }};
            ",
            file_id = c.file_id,
            runtime_pkg = c.options.runtime_package,
                proto = proto,
                deps = deps.join("")
            ));
        }

        for def in file.top_level_defs() {
            let s = c.compile_topleveldef(def)?;
            c.outer.push_str(&s);
            c.outer.push('\n');
        }

        Ok((c.outer, c.file_id))
    }

    fn resolve(&self, name_str: &str, scope: &str) -> Option<ResolvedType> {
        let typ = match self.file.pool().find_relative_type(scope, name_str) {
            Some(v) => v,
            None => return None,
        };

        let (absolute_name, file_index) = match &typ {
            TypeDescriptor::Message(d) => (d.name(), d.file_index()),
            TypeDescriptor::Enum(d) => (d.name(), d.file_index()),
            TypeDescriptor::Service(d) => todo!(),
            TypeDescriptor::Extend(_) => todo!(),
        };

        let (package_name, package_path) = {
            if file_index == self.file.index() {
                // No need to specify a package path if in the same file
                (self.file.proto().package(), "".to_string())
            } else {
                let imported_proto = self
                    .imported_protos
                    .get(&file_index)
                    // This may fail in cases where we didn't directly import the proto (transitive
                    // dependency).
                    .expect("Type not in an imported proto");

                (
                    imported_proto.proto.proto().package(),
                    format!("{}::", imported_proto.package_path),
                )
            }
        };

        let relative_name = absolute_name
            .strip_prefix(package_name)
            .and_then(|s| {
                if package_name.is_empty() {
                    Some(s)
                } else {
                    s.strip_prefix('.')
                }
            })
            .expect("Type not in its package");

        let typename = format!(
            "{}{}",
            package_path,
            relative_name
                .split('.')
                .map(|s| escape_rust_identifier(s))
                .collect::<Vec<_>>()
                .join("_")
        );

        Some(ResolvedType {
            typename,
            descriptor: typ,
        })
    }

    fn compile_enum(&self, e: &EnumDescriptor) -> Result<String> {
        if self.file.syntax() == Syntax::Proto3 {
            let mut has_default = false;

            for v in e.proto().value() {
                if v.number() == 0 {
                    has_default = true;
                    break;
                }
            }

            if !has_default {
                // TODO: Return an error.
            }
        }

        let allow_alias = e.proto().options().allow_alias();

        let mut lines = LineBuilder::new();

        // Because we can't put an enum inside of a struct in Rust, we instead
        // create a top level enum.
        let fullname = self.resolve(e.name(), "").expect("..").typename;

        let mut seen_numbers = HashMap::new();
        let mut duplicates = vec![];

        // TODO: Implement a better debug function
        lines.add("#[derive(Clone, Copy, PartialEq, Eq, Debug)]");
        lines.add(format!("pub enum {} {{", fullname));
        for v in e.proto().value() {
            if seen_numbers.contains_key(&v.number()) {
                if !allow_alias {
                    panic!("Duplicate enum value: {} in {}", v.number(), e.name());
                }

                duplicates.push(v);
                continue;
            } else {
                seen_numbers.insert(v.number(), v);
            }

            lines.add(format!(
                "\t{} = {},",
                escape_rust_identifier(&v.name()),
                v.number()
            ));
        }
        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", fullname));
        for duplicate in duplicates {
            let main_field = seen_numbers.get(&duplicate.number()).unwrap();

            lines.add(format!(
                "pub const {}: Self = Self::{};",
                escape_rust_identifier(duplicate.name()),
                escape_rust_identifier(main_field.name())
            ));
        }
        lines.add("}");
        lines.nl();

        // TODO: RE-use this above with the proto3 check.
        let mut default_option = None;
        for v in e.proto().value() {
            if v.number() == 0 || (self.file.syntax() == Syntax::Proto2) {
                default_option = Some(v.name());
                break;
            }
        }

        // TODO: Throw an error if the enum is used in a non-optional field when it
        // doesn't have a default value.
        if let Some(option_name) = default_option {
            lines.add(format!("impl core::default::Default for {} {{", fullname));
            lines.add(format!(
                "\tfn default() -> Self {{ Self::{} }}",
                option_name
            ));
            lines.add("}");
            lines.nl();

            lines.add(format!(
                "impl common::const_default::ConstDefault for {} {{",
                fullname
            ));
            lines.add(format!("\tconst DEFAULT: Self = Self::{};", option_name));
            lines.add("}");
            lines.nl();
        }

        lines.add(format!(
            r#"impl {pkg}::Enum for {name} {{
                #[cfg(feature = "alloc")]
                fn box_clone(&self) -> Box<dyn ({pkg}::Enum) + 'static> {{ 
                    Box::new(self.clone())
                }}
                
                "#,
            pkg = self.options.runtime_package,
            name = fullname
        ));
        lines.indented(|lines| {
            // TODO: Just make from_usize an Option<>
            lines.add(format!(
                "fn parse(v: {}::EnumValue) -> WireResult<Self> {{",
                self.options.runtime_package
            ));
            lines.indented(|lines| {
                lines.add("Ok(match v {");
                for v in e.proto().value() {
                    lines.add(format!(
                        "\t{} => {}::{},",
                        v.number(),
                        fullname,
                        escape_rust_identifier(v.name())
                    ));
                }

                lines.add("\t_ => { return Err(WireError::UnknownEnumVariant); }");

                lines.add("})");
            });
            lines.add("}");
            lines.nl();

            // fn parse_name(&mut self, name: &str) -> Result<()>;
            lines.add("fn parse_name(s: &str) -> WireResult<Self> {");
            lines.add("\tOk(match s {");
            for v in e.proto().value() {
                lines.add(format!(
                    "\t\t\"{}\" => Self::{},",
                    v.name(),
                    escape_rust_identifier(v.name())
                ));
            }
            lines.add("_ => { return Err(WireError::UnknownEnumVariant); }");
            lines.add("})");
            lines.add("}");

            lines.add("fn name(&self) -> &'static str {");
            lines.add("\tmatch self {");
            for v in e.proto().value() {
                if seen_numbers[&v.number()].name() != v.name() {
                    continue;
                }

                lines.add(format!(
                    "\t\tSelf::{} => \"{}\",",
                    escape_rust_identifier(v.name()),
                    v.name()
                ));
            }
            lines.add("\t}");
            lines.add("}");
            lines.nl();

            lines.add(format!(
                "fn value(&self) -> {}::EnumValue {{ *self as {}::EnumValue }}",
                self.options.runtime_package, self.options.runtime_package
            ));
            lines.nl();

            lines.add(format!(
                "fn assign(&mut self, v: {}::EnumValue) -> WireResult<()> {{",
                self.options.runtime_package
            ));
            lines.add("\t*self = Self::parse(v)?; Ok(())");
            lines.add("}");
            lines.nl();

            lines.add("fn assign_name(&mut self, s: &str) -> WireResult<()> {");
            lines.add("\t*self = Self::parse_name(s)?; Ok(())");
            lines.add("}");
            lines.nl();
        });
        lines.add("}");
        lines.nl();

        lines.add(format!(
            "impl {}::reflection::Reflect for {} {{",
            self.options.runtime_package, fullname
        ));
        lines.indented(|lines| {
            lines.add(format!("fn reflect(&self) -> {}::reflection::Reflection {{ {}::reflection::Reflection::Enum(self) }}",
                      self.options.runtime_package, self.options.runtime_package));
            lines.add(format!("fn reflect_mut(&mut self) -> {}::reflection::ReflectionMut {{ {}::reflection::ReflectionMut::Enum(self) }}",
                      self.options.runtime_package, self.options.runtime_package));
        });
        lines.add("}");

        Ok(lines.to_string())
    }

    fn compile_field_type(&self, field: &FieldDescriptor) -> Result<String> {
        self.compile_field_type_inner(field.proto(), field.message().name())
    }

    fn compile_field_type_inner(
        &self,
        proto: &protobuf_descriptor::FieldDescriptorProto,
        scope: &str,
    ) -> Result<String> {
        let max_length = *proto.options().max_length()?;

        if max_length != 0 {
            if proto.typ() == FieldDescriptorProto_Type::TYPE_BYTES {
                return Ok(format!("FixedVec<u8, {size}>", size = max_length));
            } else if proto.typ() == FieldDescriptorProto_Type::TYPE_STRING {
                return Ok(format!("FixedString<[u8; {size}]>", size = max_length));
            } else {
                return Err(err_msg("max_length not supported on type"));
            }
        }

        Ok(String::from(match proto.typ() {
            FieldDescriptorProto_Type::TYPE_DOUBLE => "f64",
            FieldDescriptorProto_Type::TYPE_FLOAT => "f32",
            FieldDescriptorProto_Type::TYPE_INT32 => "i32",
            FieldDescriptorProto_Type::TYPE_INT64 => "i64",
            FieldDescriptorProto_Type::TYPE_UINT32 => "u32",
            FieldDescriptorProto_Type::TYPE_UINT64 => "u64",
            FieldDescriptorProto_Type::TYPE_SINT32 => "i32",
            FieldDescriptorProto_Type::TYPE_SINT64 => "i64",
            FieldDescriptorProto_Type::TYPE_FIXED32 => "u32",
            FieldDescriptorProto_Type::TYPE_FIXED64 => "u64",
            FieldDescriptorProto_Type::TYPE_SFIXED32 => "i32",
            FieldDescriptorProto_Type::TYPE_SFIXED64 => "i64",
            FieldDescriptorProto_Type::TYPE_BOOL => "bool",
            FieldDescriptorProto_Type::TYPE_STRING => "String",
            FieldDescriptorProto_Type::TYPE_BYTES => "BytesField",
            FieldDescriptorProto_Type::TYPE_MESSAGE | FieldDescriptorProto_Type::TYPE_ENUM => {
                return Ok(self
                    .resolve(proto.type_name(), scope)
                    .expect(&format!(
                        "Failed to resolve field type: {}",
                        proto.type_name()
                    ))
                    .typename);
            }
            FieldDescriptorProto_Type::TYPE_GROUP => todo!(),
        }))
    }

    /// Gets an str representing the proto identifier for this type.
    /// This string is used in the name of all wire format functions so can
    /// be used for code generation.
    fn field_codec_name(&self, field: &FieldDescriptor) -> &str {
        match field.proto().typ() {
            FieldDescriptorProto_Type::TYPE_DOUBLE => "Double",
            FieldDescriptorProto_Type::TYPE_FLOAT => "Float",
            FieldDescriptorProto_Type::TYPE_INT32 => "Int32",
            FieldDescriptorProto_Type::TYPE_INT64 => "Int64",
            FieldDescriptorProto_Type::TYPE_UINT32 => "UInt32",
            FieldDescriptorProto_Type::TYPE_UINT64 => "UInt64",
            FieldDescriptorProto_Type::TYPE_SINT32 => "SInt32",
            FieldDescriptorProto_Type::TYPE_SINT64 => "SInt64",
            FieldDescriptorProto_Type::TYPE_FIXED32 => "Fixed32",
            FieldDescriptorProto_Type::TYPE_FIXED64 => "Fixed64",
            FieldDescriptorProto_Type::TYPE_SFIXED32 => "SFixed32",
            FieldDescriptorProto_Type::TYPE_SFIXED64 => "SFixed64",
            FieldDescriptorProto_Type::TYPE_BOOL => "Bool",
            FieldDescriptorProto_Type::TYPE_STRING => "String",
            FieldDescriptorProto_Type::TYPE_BYTES => "Bytes",
            FieldDescriptorProto_Type::TYPE_MESSAGE | FieldDescriptorProto_Type::TYPE_ENUM => {
                let typ = field.find_type().expect("Can't find field type");
                match typ {
                    TypeDescriptor::Message(_) => "Message",
                    TypeDescriptor::Enum(_) => "Enum",
                    _ => todo!(),
                }
            }
            _ => todo!(),
        }
    }

    fn field_name<'a>(&self, field: &'a FieldDescriptor) -> &'a str {
        escape_rust_identifier(&field.proto().name())
    }

    fn field_name_inner(name: &str) -> &str {
        escape_rust_identifier(name)
    }

    /// Checks if a type is a 'primitive'.
    ///
    /// A primitive is defined mainly as anything but a nested message type.
    /// In proto3, the presence of primitive fields is undefined.
    fn is_primitive(&self, field: &FieldDescriptor) -> Result<bool> {
        if field.proto().has_type_name() {
            let typ = field.find_type().expect(&format!(
                "Failed to resolve type: {}",
                field.proto().type_name()
            ));

            match typ {
                TypeDescriptor::Message(m) => {
                    if *m.proto().options().typed_num()? {
                        // TODO: Must check if a boolean and no duplicate options and that
                        // the boolean value is true
                        return Ok(true);
                    }
                }
                TypeDescriptor::Enum(_) => return Ok(true),
                TypeDescriptor::Service(_) => todo!(),
                TypeDescriptor::Extend(_) => todo!(),
            }

            return Ok(false);
        }

        Ok(true)
    }

    fn is_copyable(&self, field: &FieldDescriptor) -> Result<bool> {
        Ok(match field.proto().typ() {
            FieldDescriptorProto_Type::TYPE_ENUM | FieldDescriptorProto_Type::TYPE_MESSAGE => {
                let typ = field.find_type().expect(&format!(
                    "Failed to resolve type: {}",
                    field.proto().type_name()
                ));

                match typ {
                    TypeDescriptor::Enum(_) => true,
                    TypeDescriptor::Message(m) => {
                        if *m.proto().options().typed_num()? {
                            true
                        } else {
                            false
                        }
                    }
                    _ => todo!(),
                }
            }
            FieldDescriptorProto_Type::TYPE_STRING => false,
            FieldDescriptorProto_Type::TYPE_BYTES => false,
            _ => true,
        })
    }

    fn is_message(&self, field: &FieldDescriptor) -> Result<bool> {
        if field.proto().has_type_name() {
            let typ = field.find_type().expect(&format!(
                "Failed to resolve type: {}",
                field.proto().type_name()
            ));

            if let TypeDescriptor::Message(_) = typ {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn is_unordered_set(&self, field: &FieldDescriptor) -> Result<bool> {
        if field.proto().label() != FieldDescriptorProto_Label::LABEL_REPEATED {
            return Ok(false);
        }

        Ok(*field.proto().options().unordered_set()?)
    }

    fn is_map_field(&self, field: &FieldDescriptor) -> Result<Option<MapField>> {
        // TODO: Re-enable this once it is more stable.
        return Ok(None);

        if !field.proto().has_type_name() {
            return Ok(None);
        }

        let typ = field.find_type().expect(&format!(
            "Failed to resolve type: {}",
            field.proto().type_name()
        ));

        let message = match typ {
            TypeDescriptor::Message(m) => m,
            _ => return Ok(None),
        };

        if !message.proto().options().map_entry() {
            return Ok(None);
        }

        let fields = message.fields().collect::<Vec<_>>();
        if fields.len() != 2 {
            return Err(err_msg("Map field should have exactly 2 fields"));
        }

        if fields[0].proto().number() != 1 || fields[1].proto().number() != 2 {
            return Err(err_msg("Failed to find the key/value map fields"));
        }

        Ok(Some(MapField {
            field: field.clone(),
            key_field: fields[0].clone(),
            value_field: fields[1].clone(),
        }))
    }

    fn compile_field(&mut self, field: &FieldDescriptor) -> Result<String> {
        let mut s = String::new();
        s += self.field_name(field);
        s += ": ";

        let mut typ = self.compile_field_type(field)?;

        let is_repeated = field.proto().label() == FieldDescriptorProto_Label::LABEL_REPEATED;

        let max_count = *field.proto().options().max_count()?;

        // We must box raw messages if they may cause cycles. It also simplifies support
        // for dynamic messages.
        // TODO: Do the same thing for groups.
        let is_message = self.is_message(&field)?;
        if is_message && !self.is_unordered_set(field)? && !self.is_primitive(field)? {
            typ = format!("MessagePtr<{}>", typ);
        }

        /*
        Follow the nanopb convection:
        - max_length: For strings and bytes
        - max_count: For repeated fields.
        */

        if self.is_unordered_set(field)? {
            s += &format!("{}::SetField<{}>", self.options.runtime_package, typ);
        } else if is_repeated {
            if max_count != 0 {
                s += &format!("FixedVec<{typ}, {size}>", typ = typ, size = max_count);
            } else {
                s += &format!("Vec<{}>", &typ);
            }
        } else {
            if self.is_primitive(field)? && self.file.syntax() == Syntax::Proto3 {
                s += &typ;
            } else {
                s += &format!("Option<{}>", typ);
            }
        }

        s += ",";
        Ok(s)
    }

    fn oneof_typename(&self, oneof: &OneOfDescriptor) -> String {
        let message_name = self.resolve(oneof.message().name(), "").expect("...");

        message_name.typename
            + escape_rust_identifier(&common::snake_to_camel_case(&oneof.proto().name()))
            + "Case"
    }

    fn compile_oneof(&mut self, oneof: &OneOfDescriptor) -> Result<CompiledOneOf> {
        let mut lines = LineBuilder::new();

        let typename = self.oneof_typename(oneof);

        lines.add("#[derive(Clone, PartialEq)]");
        lines.add(r#"#[cfg_attr(feature = "alloc", derive(Debug))]"#);
        lines.add(format!("pub enum {} {{", typename));
        lines.add("\tNOT_SET,");
        for field in oneof.fields() {
            let mut typ = self.compile_field_type(&field)?;

            // TODO: Only do this for message types.
            if self.is_message(&field)? && !self.is_primitive(&field)? {
                typ = format!("MessagePtr<{}>", typ);
            }

            lines.add(format!(
                "\t{}({}),",
                common::snake_to_camel_case(field.proto().name()),
                typ
            ));
        }
        lines.add("}");
        lines.nl();

        lines.add(format!("impl Default for {} {{", typename));
        lines.add("\tfn default() -> Self {");
        lines.add(format!("\t\t{}::NOT_SET", typename));
        lines.add("\t}");
        lines.add("}");
        lines.nl();

        lines.add(format!(
            "impl common::const_default::ConstDefault for {} {{",
            typename
        ));
        lines.add(format!(
            "\tconst DEFAULT: {} = {}::NOT_SET;",
            typename, typename
        ));
        lines.add("}");
        lines.nl();

        Ok(CompiledOneOf {
            typename,
            source: lines.to_string(),
        })
    }

    fn compile_default_value(&self, field: &FieldDescriptor) -> Result<String> {
        let value = field.proto().default_value();

        use FieldDescriptorProto_Type::*;

        Ok(match field.proto().typ() {
            TYPE_DOUBLE | TYPE_FLOAT | TYPE_INT64 | TYPE_UINT64 | TYPE_INT32 | TYPE_FIXED64
            | TYPE_FIXED32 | TYPE_UINT32 | TYPE_SFIXED32 | TYPE_SFIXED64 | TYPE_SINT32
            | TYPE_SINT64 => {
                // TODO: Sanity check for security that the value looks like a number

                value.to_string()
            }

            TYPE_BOOL => {
                if value != "true" && value != "false" {
                    panic!("Invalid bool");
                }

                value.to_string()
            }
            TYPE_STRING => {
                let mut out = String::new();
                serialize_str_lit(value.as_bytes(), &mut out);
                out
            }
            TYPE_GROUP => todo!(),
            TYPE_MESSAGE | TYPE_ENUM => {
                // TODO: This assumes that it is an enum.
                // TODO: validate it is a valid value in the enum.

                let enum_name = self.compile_field_type(field)?;
                format!("{}::{}", enum_name, value)
            }
            TYPE_BYTES => {
                // TODO: First parse value as an str_lit it before serializing

                todo!()
            }
        })
    }

    fn compile_field_accessors(
        &self,
        field: &FieldDescriptor,
        oneof: Option<&OneOfDescriptor>,
    ) -> Result<String> {
        let mut lines = LineBuilder::new();

        let name = self.field_name(field);

        // TODO: Verify the given label is allowed in the current syntax
        // version

        // TOOD: verify that the label is handled appropriately for oneof fields.

        let is_repeated = field.proto().label() == FieldDescriptorProto_Label::LABEL_REPEATED;
        let typ = self.compile_field_type(field)?;

        let is_primitive = self.is_primitive(field)?;
        let is_copyable = self.is_copyable(field)?;
        let is_message = self.is_message(field)?;

        // NOTE: We

        let oneof_option = true; // !(is_primitive && self.proto.syntax == Syntax::Proto3);

        // TODO: Messages should always have options?
        let use_option =
            !((is_primitive && self.file.syntax() == Syntax::Proto3) || oneof.is_some());

        // field()
        if self.is_unordered_set(field)? {
            lines.add(format!(
                "
                pub fn {name}(&self) -> &{pkg}::SetField<{typ}> {{
                    &self.{name}
                }}
                ",
                name = name,
                typ = typ,
                pkg = self.options.runtime_package
            ));
        } else if is_repeated {
            let max_count = *field.proto().options().max_count()?;

            let mut typ = typ.clone();
            if is_message && !is_primitive {
                typ = format!("MessagePtr<{}>", typ);
            }

            let vec_type = {
                if max_count != 0 {
                    format!("FixedVec<{typ}, {size}>", typ = typ, size = max_count)
                } else {
                    format!("Vec<{}>", typ)
                }
            };

            lines.add(format!(
                r#"
                pub fn {name}(&self) -> &[{typ}] {{
                    &self.{name}
                }}
            
                pub fn {name}_mut(&mut self) -> &mut {vec_type} {{
                    &mut self.{name}
                }}
            "#,
                name = name,
                typ = typ,
                vec_type = vec_type
            ));
        } else {
            let modifier = if is_copyable { "" } else { "&" };

            let rettype = {
                match &field.proto().typ() {
                    FieldDescriptorProto_Type::TYPE_STRING => "str",
                    FieldDescriptorProto_Type::TYPE_BYTES => "[u8]",
                    _ => &typ,
                }
            };

            let default_value = {
                if field.proto().has_default_value() {
                    self.compile_default_value(field)?
                } else if rettype == "str" {
                    "\"\"".to_string()
                } else if rettype == "[u8]" {
                    "&[]".to_string()
                } else if is_message && !is_copyable {
                    format!("{}::static_default_value()", typ)
                } else {
                    // For now it's a const,
                    format!("{}<{}>::DEFAULT", modifier, typ)
                }
            };

            // NOTE: For primitives, it is sufficient to copy it.
            lines.add(format!(
                "\tpub fn {}(&self) -> {}{} {{",
                name, modifier, rettype
            ));
            if use_option {
                if is_copyable {
                    if field.proto().has_default_value() {
                        lines.add_inline(format!(" self.{}.unwrap_or({}) }}", name, default_value));
                    } else {
                        lines.add_inline(format!(" self.{}.unwrap_or_default() }}", name));
                    }
                } else {
                    lines.add_inline(format!(
                        " self.{}.as_ref().map(|v| v.as_ref()).unwrap_or({}) }}",
                        name, default_value
                    ));
                }
            } else if let Some(oneof) = oneof.clone() {
                let oneof_typename = self.oneof_typename(oneof);
                let oneof_fieldname = Self::field_name_inner(&oneof.proto().name());
                let oneof_case = common::snake_to_camel_case(&field.proto().name());

                // Step 1: Ensure that the enum has the right case. Else give it a default
                // value.
                lines.add_inline(format!(
                    " if let {}::{}(v) = &self.{} {{ {}{}v }} else {{ {} }} }}",
                    oneof_typename,
                    oneof_case,
                    oneof_fieldname,
                    modifier,
                    if is_copyable { "*" } else { "" },
                    default_value
                ));

                if oneof_option {
                    lines.add(format!(
                        "
                        pub fn has_{}(&self) -> bool {{
                            if let {}::{}(_) = &self.{} {{ true }} else {{ false }}
                        }}
                    ",
                        name, oneof_typename, oneof_case, oneof_fieldname
                    ));
                }
            } else {
                lines.add_inline(format!(" {}self.{} }}", modifier, name));
            }
        }

        if is_repeated {
            // field_len()
            lines.add(format!("\tpub fn {}_len(&self) -> usize {{", name));
            lines.add_inline(format!(" self.{}.len() }}", name));
        } else {
            // has_field() -> bool
            if use_option {
                lines.add(format!("\tpub fn has_{}(&self) -> bool {{", name));
                lines.add_inline(format!(" self.{}.is_some() }}", name));
            }
        }

        if self.is_unordered_set(field)? {
            lines.add(format!(
                "
                pub fn {name}_mut(&mut self) -> &mut {pkg}::SetField<{typ}> {{
                    &mut self.{name}
                }}
            ",
                name = name,
                typ = typ,
                pkg = self.options.runtime_package
            ));
        } else if is_repeated {
            // add_field(v: T) -> &mut T

            let mut inner_typ = typ.clone();
            let mut inner_v = "v".to_string();
            if is_message {
                inner_typ = format!("MessagePtr<{}>", inner_typ);
                inner_v = format!("MessagePtr::new({})", inner_v);
            }

            lines.add(format!(
                "
                pub fn add_{name}(&mut self, v: {typ}) -> &mut {inner_typ} {{
                    self.{name}.push({inner_v});
                    self.{name}.last_mut().unwrap()
                }}

                pub fn new_{name}(&mut self) -> &mut {inner_typ} {{
                    self.{name}.push(<{inner_typ}>::default());
                    self.{name}.last_mut().unwrap()
                }}
            ",
                name = name,
                typ = typ,
                inner_typ = inner_typ,
                inner_v = inner_v,
            ));

        // NOTE: We do not define 'fn add_field() -> &mut T'
        } else {
            if
            /* is_primitive */
            true {
                // set_field(v: T)
                lines.add(format!(
                    "\tpub fn set_{}<V: ::core::convert::Into<{}>>(&mut self, v: V) {{",
                    name, typ
                ));
                lines.add("\t\tlet v = v.into();");
                if use_option {
                    if is_message {
                        lines.add(format!("\t\tself.{} = Some(MessagePtr::new(v));", name));
                    } else {
                        lines.add(format!("\t\tself.{} = Some(v);", name));
                    }
                } else {
                    if let Some(oneof) = oneof.clone() {
                        let oneof_typename = self.oneof_typename(oneof);
                        let oneof_fieldname = Self::field_name_inner(&oneof.proto().name());
                        let oneof_case = common::snake_to_camel_case(&field.proto().name());

                        let val = if is_message && !self.is_primitive(field)? {
                            "MessagePtr::new(v)"
                        } else {
                            "v"
                        };

                        lines.add(format!(
                            "\t\tself.{} = {}::{}({});",
                            oneof_fieldname, oneof_typename, oneof_case, val
                        ));
                    } else {
                        lines.add(format!("\t\tself.{} = v;", name));
                    }
                }
                lines.add("\t}");
            }

            // TODO: For Option<>, must set it to be a Some(Type::default())
            // ^ Will also need to
            // field_mut() -> &mut T
            lines.add(format!(
                "\tpub fn {}_mut(&mut self) -> &mut {} {{",
                name, typ
            ));

            if use_option {
                if is_message {
                    lines.add_inline(format!(" self.{}.get_or_insert_with(|| MessagePtr::new({}::default())).as_mut() }}", name, typ));
                } else {
                    lines.add_inline(format!(
                        " self.{}.get_or_insert_with(|| <{}>::default()) }}",
                        name, typ
                    ));
                }
            } else {
                if let Some(oneof) = oneof.clone() {
                    let oneof_typename = self.oneof_typename(oneof);
                    let oneof_fieldname = Self::field_name_inner(&oneof.proto().name());
                    let oneof_case = common::snake_to_camel_case(&field.proto().name());

                    let mut typ = typ.clone();
                    if is_message && !is_primitive {
                        typ = format!("MessagePtr<{}>", typ);
                    }

                    // Step 1: Ensure that the enum has the right case. Else give it a default
                    // value.
                    lines.add(format!("\t\tif let {}::{}(_) = &self.{} {{ }} else {{ self.{} = {}::{}(<{}>::default()); }}",
                              oneof_typename, oneof_case, oneof_fieldname, oneof_fieldname, oneof_typename, oneof_case, typ));

                    // Step 2: Return the mutable reference.
                    lines.add(format!(
                        "\t\tif let {}::{}(v) = &mut self.{} {{ v }} else {{ panic!() }}",
                        oneof_typename, oneof_case, oneof_fieldname
                    ));

                    lines.add("\t}");
                } else {
                    lines.add_inline(format!(" &mut self.{} }}", name));
                }
            }
        }

        // clear_field()
        if is_repeated || use_option {
            lines.add(format!("\tpub fn clear_{}(&mut self) {{", name));
            if is_repeated {
                lines.add_inline(format!(" self.{}.clear(); }}", name));
            } else {
                lines.add_inline(format!(" self.{} = None; }}", name));
            }
        }

        Ok(lines.to_string())
    }

    fn compile_map_field_accessors(&self, field: &MapField) -> Result<String> {
        let key_type = self.compile_field_type(&field.key_field)?;
        let mut value_type = self.compile_field_type(&field.value_field)?;

        if self.is_message(&field.value_field)? {
            value_type = format!("MessagePtr<{}>", value_type);
        }

        Ok(format!(
            r#"
            pub fn {name}(&self) -> &{pkg}::MapField<{key_type}, {value_type}> {{
                &self.{name}
            }}
            
            pub fn {name}_mut(&mut self) -> &{pkg}::MapField<{key_type}, {value_type}> {{
                &mut self.{name}
            }}
            "#,
            name = escape_rust_identifier(field.field.proto().name()), // TODO: Use a nice name.
            key_type = key_type,
            value_type = value_type,
            pkg = self.options.runtime_package
        ))
    }

    fn compile_message(&mut self, msg: &MessageDescriptor) -> Result<String> {
        /*
        Supporting oneof:
        - Internally implemented as an enum to allow simply

        - Must verify that no label is specified for these fields
        - They must all also have distinct numbers.
        */

        // TODO: Create a ConstDefault version of the message (should be used if someone
        // wants to access an uninitialized message?)

        // TODO: Complain if we include a proto2 enum directly as a field of a proto3
        // message.

        // TODO: Complain about any unsupported options in fields

        // TOOD: Must validate that the typed num field contains only one field which is
        // an integer type.
        let mut is_typed_num = *msg.proto().options().typed_num()?;
        // let mut can_be_typed_num = true;

        let mut can_have_extensions = !msg.proto().extension_range().is_empty();

        let mut used_nums: HashSet<FieldNumber> = HashSet::new();
        for field in msg.fields() {
            if !used_nums.insert(field.proto().number() as FieldNumber) {
                panic!("Duplicate field number: {}", field.proto().number());
            }
            if self.file.syntax() == Syntax::Proto3 {
                // proto3 now allows optional fields again.

                // if field.label != Label::None && field.label !=
                // Label::Repeated {
                //     panic!("Invalid field label in proto3");
                // }
            } else {
                // TODO: Check this in the syntax parsing
                /*
                if field.proto().label() == FieldDescriptorProto_Label::UNKNOWN {
                    panic!("Missing field label in proto2 field");
                }
                */
            }
        }

        // TODO: Validate reserved field numbers/names.

        // TOOD: Debug with the enum code
        let mut fullname = self.resolve(msg.name(), "").expect("..").typename;

        let mut lines = LineBuilder::new();

        for e in msg.nested_enums() {
            lines.nl();
            lines.add(self.compile_enum(&e)?);
        }
        for message in msg.nested_messages() {
            lines.nl();
            lines.add(self.compile_message(&message)?);
        }
        for e in msg.nested_extensions() {
            lines.nl();
            lines.add(self.compile_extension(&e)?);
        }

        lines.add("#[derive(Clone, Default, PartialEq, ConstDefault)]");
        lines.add(format!("pub struct {} {{", fullname));
        lines.indented(|lines| -> Result<()> {
            for field in msg.fields() {
                if field.proto().has_oneof_index() {
                    continue;
                }

                let is_map_field = self.is_map_field(&field)?;
                if let Some(map_field) = is_map_field {
                    let mut value_type = self.compile_field_type(&map_field.value_field)?;
                    if self.is_message(&map_field.value_field)? {
                        value_type = format!("MessagePtr<{}>", value_type);
                    }

                    let mut s = String::new();
                    s += escape_rust_identifier(map_field.field.proto().name());
                    s += &format!(": {}::MapField<", &self.options.runtime_package);
                    s += &self.compile_field_type(&map_field.key_field)?;
                    s += ", ";
                    s += &value_type;
                    s += ">,";
                    lines.add(s);
                } else {
                    lines.add(self.compile_field(&field)?);
                }
            }

            for oneof in msg.oneofs() {
                let compiled = self.compile_oneof(&oneof)?;
                self.outer.push_str(&compiled.source);
                lines.add(format!(
                    "{}: {},",
                    Self::field_name_inner(&oneof.proto().name()),
                    compiled.typename
                ));
            }

            if can_have_extensions {
                lines.add(format!(
                    "extensions: {}::ExtensionSet,",
                    self.options.runtime_package
                ));
            } else if !is_typed_num {
                lines.add(format!(
                    "unknown_fields: {}::UnknownFieldSet,",
                    self.options.runtime_package
                ));
            }

            Ok(())
        })?;
        lines.add("}");
        lines.nl();

        lines.add(format!(
            "
            #[cfg(feature = \"alloc\")]
            impl ::core::fmt::Debug for {name} {{
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {{
                    f.write_str(&{pkg}::text::serialize_text_proto(self))
                }}
            }}
        ",
            name = fullname,
            pkg = self.options.runtime_package
        ));

        // TODO: Use text format for

        if is_typed_num {
            let field = {
                let mut field_iter = msg.fields();
                let mut f = field_iter.next();
                if field_iter.next().is_some() {
                    f = None;
                }

                f.ok_or_else(|| err_msg("Expected typed_num message to have exactly one field"))?
            };

            if field.proto().has_type_name() {
                return Err(err_msg("typed_num message field must be a primitive type"));
            }

            let field_type = self.compile_field_type(&field)?;

            lines.add(format!(
                r#"
                impl Copy for {msg_name} {{}}

                impl ::core::ops::Add<Self> for {msg_name} {{
                    type Output = Self;
                    
                    fn add(self, other: Self) -> Self {{
                        let mut sum = self.clone();
                        *sum.{field_name}_mut() += other.{field_name}();
                        sum 
                    }}
                }}

                impl ::core::ops::Add<{field_type}> for {msg_name} {{
                    type Output = Self;
                    
                    fn add(self, other: {field_type}) -> Self {{
                        let mut sum = self.clone();
                        *sum.{field_name}_mut() += other;
                        sum
                    }}
                }}

                impl ::core::ops::Sub<{field_type}> for {msg_name} {{
                    type Output = Self;
                    
                    fn sub(self, other: {field_type}) -> Self {{
                        let mut result = self.clone();
                        *result.{field_name}_mut() -= other;
                        result
                    }}
                }}

                impl Eq for {msg_name} {{}}

                impl ::core::cmp::PartialOrd for {msg_name} {{
                    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {{
                        Some(self.cmp(other))
                    }}
                }}
                
                impl Ord for {msg_name} {{
                    fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {{
                        self.{field_name}().cmp(&other.{field_name}())
                    }}
                }}
                
                impl ::core::convert::From<{field_type}> for {msg_name} {{
                    fn from(v: {field_type}) -> Self {{
                        let mut inst = Self::default();
                        inst.set_{field_name}(v);
                        inst
                    }}
                }}

                impl ::core::hash::Hash for {msg_name} {{
                    fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {{
                        self.{field_name}().hash(state);
                    }}
                }}

            "#,
                msg_name = fullname,
                field_type = field_type,
                field_name = escape_rust_identifier(&field.proto().name())
            ));
        }

        lines.add(format!("impl {} {{", fullname));

        {
            for field in msg.fields() {
                lines.add(format!(
                    "pub const {field_name}_FIELD_NUM: {pkg}::FieldNumber = {num};",
                    field_name = escape_rust_identifier(field.proto().name()).to_uppercase(),
                    pkg = self.options.runtime_package,
                    num = field.proto().number()
                ));
            }

            lines.nl();
        }

        lines.add("\tpub fn static_default_value() -> &'static Self {");
        lines.add(format!(
            "\t\tstatic VALUE: {} = {}::DEFAULT;",
            fullname, fullname
        ));
        lines.add("\t\t&VALUE");
        lines.add("\t}");
        lines.nl();

        for field in msg.fields() {
            if field.proto().has_oneof_index() {
                continue;
            }

            if let Some(map_field) = self.is_map_field(&field)? {
                lines.add(self.compile_map_field_accessors(&map_field)?);
            } else {
                lines.add(self.compile_field_accessors(&field, None)?);
            }
        }

        for oneof in msg.oneofs() {
            let oneof_typename = self.oneof_typename(&oneof);
            lines.add(format!(
                "
                pub fn {name}_case(&self) -> &{ty} {{ &self.{field} }}
                pub fn {name}_case_mut(&mut self) -> &mut {ty} {{ &mut self.{field} }}
                ",
                name = escape_rust_identifier(oneof.proto().name()),
                ty = oneof_typename,
                field = Self::field_name_inner(&oneof.proto().name())
            ));

            for field in oneof.fields() {
                lines.add(self.compile_field_accessors(&field, Some(&oneof))?);
            }

            // Should also add fields to assign to each of the items
        }

        lines.add("}");
        lines.nl();

        lines.add(format!(
            r#"
            impl {runtime_pkg}::StaticMessage for {name} {{
                #[cfg(feature = "std")]
                fn file_descriptor() -> &'static {runtime_pkg}::StaticFileDescriptor {{
                    &FILE_DESCRIPTOR_{file_id}
                }}
            }}
        "#,
            name = fullname,
            runtime_pkg = self.options.runtime_package,
            file_id = self.file_id,
        ));

        lines.add(format!(
            r#"impl {pkg}::Message for {name} {{
                
                fn type_url(&self) -> &str {{
                    "{type_url}"
                }}
                
                #[cfg(feature = "alloc")]
                fn serialize(&self) -> Result<Vec<u8>> {{
                    let mut data = Vec::new();
                    self.serialize_to(&mut data)?;
                    Ok(data)
                }}

                #[cfg(feature = "alloc")]
                fn merge_from(&mut self, other: &Self) -> Result<()>
                where
                    Self: Sized
            
                {{
                    use {pkg}::ReflectMergeFrom;
                    self.reflect_merge_from(other)
                }}

                #[cfg(feature = "alloc")]
                fn box_clone(&self) -> Box<dyn ({pkg}::Message) + 'static> {{ 
                    Box::new(self.clone())
                }}

                "#,
            type_url = msg.type_url(),
            pkg = self.options.runtime_package,
            name = fullname
        ));

        lines.add("\tfn parse_merge(&mut self, data: &[u8]) -> WireResult<()> {");
        lines.add("\t\tfor field_ref in WireFieldIter::new(data) {");
        lines.add("\t\t\tlet field_ref = field_ref?;");
        lines.add("\t\t\tlet f = field_ref.field;");
        lines.add("\t\t\tmatch f.field_number {");

        // TODO: Must also iterate over maps and oneofs.
        for field in msg.fields() {
            if field.proto().has_oneof_index() {
                continue;
            }

            let name = self.field_name(&field);
            let is_repeated = field.proto().label() == FieldDescriptorProto_Label::LABEL_REPEATED;

            let use_option = !(self.is_primitive(&field)? && self.file.syntax() == Syntax::Proto3);

            let is_message = self.is_message(&field)?;

            // TODO: Deduplicate this logic.
            let typeclass = self.field_codec_name(&field);

            let is_map_field = self.is_map_field(&field)?;

            // TODO: Must use repeated variants here.
            let mut p = String::new();
            if self.is_unordered_set(&field)? {
                let mut value = "v?".to_string();
                if is_message && use_option {
                    value = format!("MessagePtr::new({})", value);
                }

                p += &format!(
                    "
                    for v in {typeclass}Codec::parse_repeated(&f) {{
                        self.{name}.insert({value});
                    }}
                    ",
                    name = name,
                    typeclass = typeclass,
                    value = value
                );
            } else if is_map_field.is_some() {
                let kv_struct_name = self
                    .resolve(field.proto().type_name(), field.message().name())
                    .unwrap()
                    .typename;

                // TODO: Only use unwrap_or_default if the value type
                p += &format!(
                    "
                    for v in MessageCodec::<{inner_name}>::parse_repeated(&f) {{
                        let v = v?;
                        self.{name}.insert(v.key, v.value.unwrap_or_default());
                    }}
                ",
                    name = name,
                    inner_name = kv_struct_name
                );
            } else if is_repeated {
                let mut value = "v?".to_string();
                if is_message && use_option {
                    value = format!("MessagePtr::new({})", value);
                }

                p += &format!(
                    "
                    for v in {typeclass}Codec::parse_repeated(&f) {{
                        self.{name}.push({value});
                    }}
                    ",
                    name = name,
                    typeclass = typeclass,
                    value = value,
                );
            } else {
                if use_option {
                    if is_message {
                        // TODO: also need to do this which not using optional but we are in
                        // proto2 required mode?
                        p += &format!(
                            "self.{} = Some(MessagePtr::new({}Codec::parse(&f)?))",
                            name, typeclass
                        );
                    } else {
                        p += &format!("self.{} = Some({}Codec::parse(&f)?)", name, typeclass);
                    }
                } else {
                    p += &format!("self.{} = {}Codec::parse(&f)?", name, typeclass);
                }
            }

            lines.add(format!(
                "\t\t\t\t{} => {{ {} }},",
                field.proto().number(),
                p
            ));
        }

        for oneof in msg.oneofs() {
            let oneof_typename = self.oneof_typename(&oneof);
            let oneof_fieldname = Self::field_name_inner(&oneof.proto().name());

            for field in oneof.fields() {
                let oneof_case = common::snake_to_camel_case(&field.proto().name());

                // TODO: Dedup with above
                let typeclass = self.field_codec_name(&field);

                let mut value = format!("{}Codec::parse(&f)?", typeclass);
                if typeclass == "Message" && !self.is_primitive(&field)? {
                    value = format!("MessagePtr::new(MessageCodec::parse(&f)?)");
                }

                lines.add(format!(
                    "{field_num} => {{ self.{oneof_fieldname} = {oneof_typename}::{oneof_case}({value}); }},",
                    field_num = field.proto().number(),
                    oneof_fieldname = oneof_fieldname,
                    oneof_typename = oneof_typename,
                    oneof_case = oneof_case,
                    value = value
                ));
            }
        }

        // TODO: Will need to record this as an unknown field.
        // TODO: Attempt to add to an extension if it exists already. (also do this in
        // the dynamic case).
        if can_have_extensions {
            lines.add(
                r#"
                _ => {
                    self.extensions.parse_merge(field_ref.span.into());
                }
            "#,
            );
        } else if !is_typed_num {
            lines.add(
                r#"
                _ => {
                    self.unknown_fields.fields.push(field_ref.span.into());
                }
            "#,
            );
        } else {
            lines.add(
                r#"
                _ => {
                    return Err(WireError::UnknownFieldsDropped);
                }
            "#,
            );
        }

        lines.add("\t\t\t}");
        lines.add("\t\t}");
        lines.add("\t\tOk(())");
        lines.add("\t}");

        lines
            .add("\tfn serialize_to<A: Appendable<Item = u8> + ?Sized>(&self, out: &mut A) -> Result<()> {");

        // TODO: Sort the serialization by the field numbers so that we get nice cross
        // version compatible formats.

        // TODO: Need to implement packed serialization/deserialization.
        for field in msg.fields() {
            if field.proto().has_oneof_index() {
                continue;
            }

            let name = self.field_name(&field);
            let is_repeated = field.proto().label() == FieldDescriptorProto_Label::LABEL_REPEATED;
            let is_message = self.is_message(&field)?;

            // TODO: Dedup with above
            let typeclass = self.field_codec_name(&field);

            let pass_reference = match field.proto().typ() {
                FieldDescriptorProto_Type::TYPE_STRING => true,
                FieldDescriptorProto_Type::TYPE_MESSAGE | FieldDescriptorProto_Type::TYPE_ENUM => {
                    true
                }
                FieldDescriptorProto_Type::TYPE_BYTES => true,
                _ => false,
            };

            let use_option = !(self.is_primitive(&field)? && self.file.syntax() == Syntax::Proto3)
                && !is_repeated;

            // TODO: We no longer need the special cases for repeated values here.
            let serialize_method = {
                // TODO: Should also check that we aren't using a 'required' label?
                if !use_option && !is_repeated {
                    format!("{}Codec::serialize_sparse", typeclass)
                } else {
                    format!("{}Codec::serialize", typeclass)
                }
            };

            let given_reference: bool = is_repeated || use_option;

            let reference_str = {
                if is_message && use_option {
                    ""
                } else if pass_reference {
                    if given_reference {
                        ""
                    } else {
                        "&"
                    }
                } else {
                    if given_reference {
                        "*"
                    } else {
                        ""
                    }
                }
            };

            let post_reference_str = {
                if is_message && use_option {
                    // Deref the MessagePtr
                    ".as_ref()"
                } else {
                    ""
                }
            };

            let serialize_line = format!(
                "\t\t\t{}({}, {}v{}, out)?;",
                serialize_method,
                field.proto().number(),
                reference_str,
                post_reference_str
            );

            if is_repeated {
                if self.is_unordered_set(&field)? {
                    // TODO: Support packed serialization of a SetField.
                    lines.add(format!(
                        "
                    for v in self.{name}.iter() {{
                        {typeclass}Codec::serialize({field_num}, {ref_str}v{post_str}, out)?;
                    }}
                    ",
                        typeclass = typeclass,
                        name = name,
                        field_num = field.proto().number(),
                        ref_str = reference_str,
                        post_str = post_reference_str,
                    ));
                } else if self.is_map_field(&field)?.is_some() {
                    let kv_struct_name = self
                        .resolve(field.proto().type_name(), field.message().name())
                        .unwrap()
                        .typename;

                    // TODO: Optimize this serialization.
                    lines.add(format!(
                        "
                        for (k, v) in self.{name}.entries() {{
                            let mut e = {inner_name}::default();
                            e.set_key(k);
                            e.set_value(v.as_ref().clone());
                            MessageCodec::serialize({field_num}, &e, out)?;
                        }}
                    ",
                        name = name,
                        inner_name = kv_struct_name,
                        field_num = field.proto().number()
                    ))
                } else {
                    lines.add(format!(
                        "{typeclass}Codec::serialize_repeated({field_num}, &self.{name}, out)?;",
                        typeclass = typeclass,
                        name = name,
                        field_num = field.proto().number()
                    ));
                }
            } else {
                // TODO: For proto3, the requirement is that it is not equal to
                // the default value (and there would be no optional for
                if use_option {
                    lines.add(format!("\t\tif let Some(v) = self.{}.as_ref() {{", name));
                    if is_message {
                        lines.add(format!(
                            "\t\tMessageCodec::serialize({}, v.as_ref(), out)?;",
                            field.proto().number()
                        ));
                    } else {
                        lines.add(serialize_line);
                    }
                    lines.add("\t\t}");
                } else {
                    // TODO: Should borrow the value when using messages
                    lines.add(format!(
                        "\t\t{}({}, {}self.{}{}, out)?;",
                        serialize_method,
                        field.proto().number(),
                        reference_str,
                        name,
                        post_reference_str
                    ));
                }

                if field.proto().label() == FieldDescriptorProto_Label::LABEL_REQUIRED {
                    lines.add_inline(" else {");
                    // TODO: Verify the field name doesn't have any quotes in it
                    lines.add(format!(
                        "\treturn Err(MessageSerializeError::RequiredFieldNotSet.into());"
                    ));
                    lines.add("}");
                }
            }
        }

        for oneof in msg.oneofs() {
            let oneof_typename = self.oneof_typename(&oneof);
            let oneof_fieldname = Self::field_name_inner(&oneof.proto().name());

            lines.add(format!("\t\tmatch &self.{} {{", oneof_fieldname));

            for field in oneof.fields() {
                let oneof_case = common::snake_to_camel_case(&field.proto().name());

                // TODO: Dedup with above
                let typeclass = self.field_codec_name(&field);

                // TODO: Deduplicate with above.
                let pass_reference = match field.proto().typ() {
                    FieldDescriptorProto_Type::TYPE_STRING => true,
                    FieldDescriptorProto_Type::TYPE_MESSAGE
                    | FieldDescriptorProto_Type::TYPE_ENUM => true,
                    FieldDescriptorProto_Type::TYPE_BYTES => true,
                    _ => false,
                };

                let mut reference = if pass_reference { "" } else { "*" };

                // Need to convert the &MessagePtr<Message> to an &Message
                let post_reference = if typeclass == "Message" && !self.is_primitive(&field)? {
                    ".as_ref()"
                } else {
                    ""
                };

                lines.add(format!(
                    "\t\t\t{oneof_typename}::{oneof_case}(v) => {{
                        {typeclass}Codec::serialize({field_num}, {reference}v{post_reference}, out)?; }}",
                    oneof_typename = oneof_typename,
                    oneof_case = oneof_case,
                    typeclass = typeclass,
                    reference = reference,
                    post_reference = post_reference,
                    field_num = field.proto().number()
                ));
            }

            lines.add(format!("\t\t\t{}::NOT_SET => {{}}", oneof_typename));

            lines.add("\t\t}");
        }

        if can_have_extensions {
            lines.add(r#"self.extensions.serialize_to(out)?;"#);
        } else if !is_typed_num {
            lines.add(r#"self.unknown_fields.serialize_to(out)?;"#);
        }

        lines.add("\t\tOk(())");
        lines.add("\t}");

        // Implementing merge_from
        // extend_from_slice and assignment

        lines.add("}"); // End of impl Message
        lines.nl();

        lines.add(format!(
            r#"#[cfg(feature = "alloc")]
             impl {pkg}::MessageReflection for {name} {{
                
                fn unknown_fields(&self) -> Option<&{pkg}::UnknownFieldSet> {{
                    {unknown_fields}
                }}


                fn extensions(&self) -> Option<&{pkg}::ExtensionSet> {{
                    {extensions}
                }}

                fn extensions_mut(&mut self) -> Option<&mut {pkg}::ExtensionSet> {{
                    {extensions_mut}
                }}

                fn box_clone2(&self) -> Box<dyn ({pkg}::MessageReflection) + 'static> {{ 
                    Box::new(self.clone())
                }}

                 "#,
            pkg = self.options.runtime_package,
            name = fullname,
            unknown_fields = {
                if can_have_extensions {
                    "Some(&self.extensions.unknown_fields())"
                } else if !is_typed_num {
                    "Some(&self.unknown_fields)"
                } else {
                    "None"
                }
            },
            extensions = {
                if can_have_extensions {
                    "Some(&self.extensions)"
                } else {
                    "None"
                }
            },
            extensions_mut = if can_have_extensions {
                "Some(&mut self.extensions)"
            } else {
                "None"
            }
        ));

        lines.indented(|lines| {
            lines.add(format!(
                "fn fields(&self) -> &[{}::FieldDescriptorShort] {{",
                self.options.runtime_package
            ));

            let mut all_fields = vec![];
            for field in msg.proto().field() {
                all_fields.push((field.number(), field.name()));
            }

            let field_strs = all_fields
                .into_iter()
                .map(|(num, name)| {
                    format!(
                        "{pkg}::FieldDescriptorShort {{ name: {pkg}::StringPtr::Static(\"{name}\"), number: {num} }}",
                        pkg = self.options.runtime_package, name = name, num = num
                    )
                })
                .collect::<Vec<_>>();
            lines.add(format!("\t&[{}]", field_strs.join(", ")));
            lines.add("}");
        });

        lines.indented(|lines| -> Result<()> {
            lines.add("fn field_by_number(&self, num: FieldNumber) -> Option<Reflection> {");
            lines.indented(|lines| {
                if msg.proto().field_len() == 0 {
                    lines.add("None");
                    return;
                }

                lines.add("match num {");

                for field in msg.fields() {
                    if field.proto().has_oneof_index() {
                        continue;
                    }

                    let name = self.field_name(&field);

                    let f = match self.file.syntax() {
                        Syntax::Proto2 => "reflect_field_proto2",
                        Syntax::Proto3 => "reflect_field_proto3",
                    };

                    lines.add(format!(
                        "\t{} => self.{}.{}(),",
                        field.proto().number(),
                        name,
                        f
                    ));
                }

                for oneof in msg.oneofs() {
                    let name = Self::field_name_inner(oneof.proto().name());

                    // TODO: The issue with this is that we can't distinguish between an
                    // invalid field and an unpopulated
                    for field in oneof.fields() {
                        lines.add(format!("\t{} => {{", field.proto().number()));
                        lines.add(format!(
                            "\t\tif let {}::{}(v) = &self.{} {{",
                            self.oneof_typename(&oneof),
                            common::snake_to_camel_case(&field.proto().name()),
                            name
                        ));
                        lines.add("\t\t\tSome(v.reflect())");

                        // TODO: Reflect a DEFAULT value
                        lines.add("\t\t} else { None }");
                        lines.add("\t}");
                    }
                }

                lines.add("\t_ => None");
                lines.add("}");
            });
            lines.add("}");
            lines.nl();

            // TODO: Dedup with the last case.
            lines.add(
                "fn field_by_number_mut(&mut self, num: FieldNumber) -> Option<ReflectionMut> {",
            );
            lines.indented(|lines| -> Result<()> {
                if msg.proto().field_len() == 0 {
                    lines.add("None");
                    return Ok(());
                }

                lines.add("Some(match num {");

                for field in msg.fields() {
                    if field.proto().has_oneof_index() {
                        continue;
                    }

                    // TODO: Implement handling of None

                    let name = self.field_name(&field);

                    let f = match self.file.syntax() {
                        Syntax::Proto2 => "reflect_field_mut_proto2",
                        Syntax::Proto3 => "reflect_field_mut_proto3",
                    };

                    lines.add(format!(
                        "\t{} => self.{}.{}(),",
                        field.proto().number(),
                        name,
                        f
                    ));
                }

                for oneof in msg.oneofs() {
                    let name = Self::field_name_inner(&oneof.proto().name());

                    for field in oneof.fields() {
                        lines.add(format!("\t{} => {{", field.proto().number()));
                        lines.add(format!(
                            "\t\tif let {}::{}(v) = &mut self.{} {{}}",
                            self.oneof_typename(&oneof),
                            common::snake_to_camel_case(&field.proto().name()),
                            name
                        ));
                        lines.add("\t\telse {");

                        let mut typ = self.compile_field_type(&field)?;

                        let is_message = self.is_message(&field)?;
                        if is_message && !self.is_primitive(&field)? {
                            typ = format!("MessagePtr<{}>", typ);
                        }

                        lines.add(format!(
                            "\t\t\tself.{} = {}::{}(<{}>::default());",
                            name,
                            self.oneof_typename(&oneof),
                            common::snake_to_camel_case(&field.proto().name()),
                            typ
                        ));
                        lines.add("\t\t}");
                        lines.nl();

                        lines.add(format!(
                            "\t\tif let {}::{}(v) = &mut self.{} {{",
                            self.oneof_typename(&oneof),
                            common::snake_to_camel_case(field.proto().name()),
                            name
                        ));

                        lines.add("\t\t\tv.reflect_mut()");

                        lines.add("\t\t} else {");
                        lines.add("\t\t\tpanic!();");
                        lines.add("\t\t}");
                        lines.add("\t}");
                    }
                }

                lines.add("\t_ => { return None; }");
                lines.add("})");

                Ok(())
            })?;
            lines.add("}");
            lines.nl();

            lines.add("fn field_number_by_name(&self, name: &str) -> Option<FieldNumber> {");
            lines.indented(|lines| {
                if msg.proto().field_len() == 0 {
                    lines.add("None");
                    return;
                }

                lines.add("Some(match name {");
                for field in msg.proto().field() {
                    lines.add(format!("\t\"{}\" => {},", field.name(), field.number()));
                }

                lines.add("\t_ => { return None; }");
                lines.add("})");
            });
            lines.add("}");

            Ok(())
        })?;

        lines.add("}");

        Ok(lines.to_string())
    }

    // TODO: 'path' will always be empty?
    fn compile_service(&mut self, service: &ServiceDescriptor) -> Result<String> {
        //		let modname = common::camel_to_snake_case(&service.name);

        let mut lines = LineBuilder::new();

        // Full name of the service including the package name
        // e.g. google.api.MyService
        let absolute_name = service.name();

        lines.add(format!(
            r#"
            #[derive(Clone)]
            pub struct {service_name}Stub {{
                channel: Arc<dyn {rpc_package}::Channel>

            }}
        "#,
            service_name = service.proto().name(),
            rpc_package = self.options.rpc_package
        ));

        lines.add(format!("impl {}Stub {{", service.proto().name()));
        lines.indented(|lines| {
            lines.add(format!("
                pub fn new(channel: Arc<dyn {rpc_package}::Channel>) -> Self {{
                    Self {{ channel }}
                }}
            ", rpc_package = self.options.rpc_package));

            for method in service.methods() {
                let req_type = self
                    .resolve(method.proto().input_type(), service.name())
                    .expect(&format!("Failed to find {}", method.proto().input_type()));
                let res_type = self.resolve(method.proto().output_type(), service.name())
                    .expect(&format!("Failed to find {}", method.proto().output_type()));

                if method.proto().client_streaming() && method.proto().server_streaming() {
                    // Bi-directional streaming

                    lines.add(format!(r#"
                        pub async fn {rpc_name}(&self, request_context: &{rpc_package}::ClientRequestContext)
                            -> ({rpc_package}::ClientStreamingRequest<{req_type}>, {rpc_package}::ClientStreamingResponse<{res_type}>) {{
                            self.channel.call_stream_stream("{service_name}", "{rpc_name}", request_context).await
                        }}"#,
                        rpc_package = self.options.rpc_package,
                        service_name = absolute_name,
                        rpc_name = method.proto().name(),
                        req_type = req_type.typename,
                        res_type = res_type.typename
                    ));
                } else if method.proto().client_streaming() {
                    // Client streaming

                    lines.add(format!(r#"
                        pub async fn {rpc_name}(&self, request_context: &{rpc_package}::ClientRequestContext)
                            -> {rpc_package}::ClientStreamingCall<{req_type}, {res_type}> {{
                            self.channel.call_stream_unary("{service_name}", "{rpc_name}", request_context).await
                        }}"#,
                        rpc_package = self.options.rpc_package,
                        service_name = absolute_name,
                        rpc_name = method.proto().name(),
                        req_type = req_type.typename,
                        res_type = res_type.typename
                    ));
                } else if method.proto().server_streaming() {
                    // Server streaming

                    lines.add(format!(r#"
                        pub async fn {rpc_name}(&self, request_context: &{rpc_package}::ClientRequestContext, request_value: &{req_type})
                            -> {rpc_package}::ClientStreamingResponse<{res_type}> {{
                            self.channel.call_unary_stream("{service_name}", "{rpc_name}", request_context, request_value).await
                        }}"#,
                        rpc_package = self.options.rpc_package,
                        service_name = absolute_name,
                        rpc_name = method.proto().name(),
                        req_type = req_type.typename,
                        res_type = res_type.typename
                    ));
                } else {
                    // Completely unary

                    lines.add(format!(r#"
                        pub async fn {rpc_name}(&self, request_context: &{rpc_package}::ClientRequestContext, request_value: &{req_type})
                            -> {rpc_package}::ClientResponse<{res_type}> {{
                            self.channel.call_unary_unary("{service_name}", "{rpc_name}", request_context, request_value).await
                        }}"#,
                        rpc_package = self.options.rpc_package,
                        service_name = absolute_name,
                        rpc_name = method.proto().name(),
                        req_type = req_type.typename,
                        res_type = res_type.typename
                    ));
                }

                lines.nl();
            }
        });
        lines.add("}");
        lines.nl();

        lines.add("#[async_trait]");
        lines.add(format!(
            "pub trait {}Service: Send + Sync + 'static {{",
            service.proto().name()
        ));

        for method in service.methods() {
            let req_type = self
                .resolve(method.proto().input_type(), service.name())
                .expect(&format!("Failed to find {}", method.proto().input_type()));
            let res_type = self
                .resolve(method.proto().output_type(), service.name())
                .expect(&format!("Failed to find {}", method.proto().output_type()));

            // TODO: Must resolve the typename.
            // TODO: I don't need to make the response '&mut' if I am giving a stream.
            lines.add(format!(
                "\tasync fn {rpc_name}(&self, request: {rpc_package}::Server{req_stream}Request<{req_type}>,
                                       response: &mut {rpc_package}::Server{res_stream}Response<{res_type}>) -> Result<()>;",
                rpc_package = self.options.rpc_package,
                rpc_name = method.proto().name(),
                req_type = req_type.typename,
                req_stream = if method.proto().client_streaming() { "Stream" } else { "" },
                res_type = res_type.typename,
                res_stream = if method.proto().server_streaming() { "Stream" } else { "" },
            ));
        }

        lines.nl();

        lines.add("}");
        lines.nl();

        // TODO: Anything that is already clone-able should not have to be wrapped.
        lines.add(format!(
            "
            pub trait {service_name}IntoService {{
                fn into_service(self) -> Arc<dyn {rpc_package}::Service>;
            }}

            impl<T: {service_name}Service> {service_name}IntoService for T {{
                fn into_service(self) -> Arc<dyn {rpc_package}::Service> {{
                    Arc::new({service_name}ServiceCaller {{
                        inner: self
                    }})
                }}
            }}

            pub struct {service_name}ServiceCaller<T> {{
                inner: T
            }}

        ",
            rpc_package = self.options.rpc_package,
            service_name = service.proto().name()
        ));

        // lines.add(format!(
        //     "pub struct {}ServiceCaller {{ inner: Box<dyn {}Service> }}",
        //     service.name, service.name
        // ));
        // lines.nl();

        lines.add("#[async_trait]");
        lines
            .add(format!(
            "impl<T: {service_name}Service> {rpc_package}::Service for {service_name}ServiceCaller<T> {{",
            rpc_package = self.options.rpc_package, service_name = service.proto().name()));
        lines.indented(|lines| {
            // TODO: Escape the string if needed.
            lines.add(format!(
                "fn service_name(&self) -> &'static str {{ \"{}\" }}",
                absolute_name
            ));

            lines.add(format!(
                "fn file_descriptor(&self) -> &'static {}::StaticFileDescriptor {{ &FILE_DESCRIPTOR_{} }}",
                self.options.runtime_package, self.file_id
            ));

            lines.add("fn method_names(&self) -> &'static [&'static str] {");
            lines.add("\t&[");

            // NOTE: We do not support the support streams feature of proto2
            // TODO: Ensure no streams are defined unless in proto2 mode.
            for method in service.methods() {
                lines.add_inline(format!("\"{}\", ", method.proto().name()));
            }
            lines.add_inline("]");
            lines.add("}");
            lines.nl();

            lines.add(format!(
                "async fn call<'a>(&self, method_name: &str, \
                        request: {rpc_package}::ServerStreamRequest<()>,
                        mut response: {rpc_package}::ServerStreamResponse<'a, ()> \
                    ) -> Result<()> {{"
                , rpc_package = self.options.rpc_package,)
            );


            lines.indented(|lines| {
                lines.add("match method_name {");

                for method in service.methods() {
                    let req_type = self
                        .resolve(method.proto().input_type(), service.name())
                        .expect(&format!("Failed to find {}", method.proto().input_type()));
                    let res_type = self
                        .resolve(method.proto().output_type(), service.name())
                        .expect(&format!("Failed to find {}", method.proto().output_type()));

                    let request_obj = {
                        if method.proto().client_streaming() {
                            format!("request.into::<{}>()", req_type.typename)
                        } else {
                            format!("request.into_unary::<{}>().await?", req_type.typename)
                        }
                    };

                    let response_obj = {
                        if method.proto().server_streaming() {
                            format!("response.into::<{}>()", res_type.typename)
                        } else {
                            format!("response.new_unary::<{}>()", res_type.typename)
                        }
                    };

                    let response_post = {
                        if method.proto().server_streaming() {
                            ""
                        } else {
                            // TODO: Make the T in into<T> more explicit.
                            "let response_value = response_obj.into_value(); response.into().send(response_value).await?;"
                        }
                    };

                    // TODO: Resolve type names.
                    // TODO: Must normalize these names to valid Rust names
                    lines.add(format!(r#"
                        "{rpc_name}" => {{
                            let request = {request};
                            let mut response_obj = {response_obj};

                            self.inner.{rpc_name}(request, &mut response_obj).await?;

                            {response_post}

                            Ok(())
                        }},"#,
                        rpc_name = method.proto().name(),
                        request = request_obj,
                        response_obj = response_obj,
                        response_post = response_post
                    ));
                }

                lines.add(format!("\t_ => Err({rpc_package}::Status::invalid_argument(format!(\"Invalid method: {{}}\", method_name)).into())", rpc_package = self.options.rpc_package,));
                lines.add("}");
            });

            lines.add("}");
        });
        lines.add("}");
        lines.nl();

        // impl Service for MyService {

        Ok(lines.to_string())
    }

    fn compile_extension(&mut self, extension: &ExtendDescriptor) -> Result<String> {
        // TODO: Verify that we are extending a message (probably have this logic in the
        // descriptor pool).
        let extendee_typename = self
            .resolve(extension.proto().extendee(), extension.name())
            .ok_or_else(|| err_msg("Failed to find extendee"))?
            .typename;

        let field_name = escape_rust_identifier(extension.proto().name());

        use protobuf_core::SingularValue;
        use protobuf_descriptor::FieldDescriptorProto_Type::*;

        let is_repeated = extension.proto().label()
            == protobuf_descriptor::FieldDescriptorProto_Label::LABEL_REPEATED;

        let mut field_type = self.compile_field_type_inner(extension.proto(), extension.name())?;
        if is_repeated {
            field_type = format!("Vec<{}>", field_type);
        }

        let trait_name = format!(
            "{}Extension",
            common::snake_to_camel_case(extension.proto().name())
        );

        let tag_name = format!("{}_EXTENSION_TAG", extension.proto().name().to_uppercase());

        if is_repeated && extension.proto().has_type_name() {
            // Still very poorly supported cases.
            return Ok("".to_string());
        }

        // TODO: Handle the default_value option.

        let default_value = match extension.proto().typ() {
            TYPE_DOUBLE => "SingularValue::Double(0.0)".to_string(),
            TYPE_FLOAT => "SingularValue::Float(0.0)".to_string(),
            TYPE_INT64 => "SingularValue::Int64(0)".to_string(),
            TYPE_UINT64 => "SingularValue::UInt64(0)".to_string(),
            TYPE_INT32 => "SingularValue::Int32(0)".to_string(),
            TYPE_FIXED64 => "SingularValue::UInt64(0)".to_string(),
            TYPE_FIXED32 => "SingularValue::UInt32(0)".to_string(),
            TYPE_BOOL => "SingularValue::Bool(false)".to_string(),
            TYPE_STRING => "SingularValue::String(String::new())".to_string(),
            TYPE_GROUP => {
                todo!()
            }
            TYPE_MESSAGE | TYPE_ENUM => {
                let r = self
                    .resolve(extension.proto().type_name(), extension.name())
                    .unwrap();
                match r.descriptor {
                    TypeDescriptor::Message(_) => {
                        format!(
                            "SingularValue::Message(Box::new({}::default()))",
                            r.typename
                        )
                    }
                    TypeDescriptor::Enum(_) => {
                        format!("SingularValue::Enum(Box::new({}::default()))", r.typename)
                    }
                    TypeDescriptor::Service(_) => todo!(),
                    TypeDescriptor::Extend(_) => todo!(),
                }
            }
            TYPE_BYTES => "SingularValue::Bytes(Vec::new().into())".to_string(),
            TYPE_UINT32 => "SingularValue::UInt32(0)".to_string(),
            TYPE_SFIXED32 => "SingularValue::Int32(0)".to_string(),
            TYPE_SFIXED64 => "SingularValue::Int64(0)".to_string(),
            TYPE_SINT32 => "SingularValue::Int32(0)".to_string(),
            TYPE_SINT64 => "SingularValue::Int64(0)".to_string(),
        };

        let default_value = format!(
            "{pkg}::Value::new({default_value}, {is_repeated})",
            pkg = self.options.runtime_package,
            default_value = default_value,
            is_repeated = is_repeated
        );

        let value_case = match extension.proto().typ() {
            TYPE_DOUBLE => "Double",
            TYPE_FLOAT => "Float",
            TYPE_INT64 => "Int64",
            TYPE_UINT64 => "UInt64",
            TYPE_INT32 => "Int32",
            TYPE_FIXED64 => "UInt64",
            TYPE_FIXED32 => "UInt32",
            TYPE_BOOL => "Bool",
            TYPE_STRING => "String",
            TYPE_GROUP => {
                todo!()
            }
            TYPE_MESSAGE | TYPE_ENUM => {
                let r = self
                    .resolve(extension.proto().type_name(), extension.name())
                    .unwrap();
                match r.descriptor {
                    TypeDescriptor::Message(_) => "Message",
                    TypeDescriptor::Enum(_) => "Enum",
                    TypeDescriptor::Service(_) => todo!(),
                    TypeDescriptor::Extend(_) => todo!(),
                }
            }
            TYPE_BYTES => "Bytes",
            TYPE_UINT32 => "UInt32",
            TYPE_SFIXED32 => "Int32",
            TYPE_SFIXED64 => "Int64",
            TYPE_SINT32 => "Int32",
            TYPE_SINT64 => "Int64",
        };

        let value_case = {
            if is_repeated {
                format!(
                    "Value::Repeated(RepeatedValues::{} {{ values: v, .. }})",
                    value_case
                )
            } else {
                format!("Value::Singular(SingularValue::{}(v))", value_case)
            }
        };

        let value_get_ref = {
            if extension.proto().has_type_name() {
                "v.as_any().downcast_ref().ok_or(WireError::BadDescriptor)?"
            } else {
                "v"
            }
        };

        // 'v' is an owned 'Value' type
        let value_get_owned = {
            if extension.proto().has_type_name() {
                "ExtensionRef::Boxed(v.into_any().downcast().map_err(|_| WireError::BadDescriptor)?)"
            } else {
                "ExtensionRef::Owned(v)"
            }
        };

        let value_get_mut = {
            if extension.proto().has_type_name() {
                "v.as_mut_any().downcast_mut().ok_or(WireError::BadDescriptor)?"
            } else {
                "v"
            }
        };

        Ok(format!(
            r#"
            struct {tag_name} {{}}

            impl {pkg}::ExtensionTag for {tag_name} {{
                fn extension_number(&self) -> {pkg}::ExtensionNumberType {{
                    {extension_number}
                }}

                fn extension_name(&self) -> {pkg}::StringPtr {{
                    {pkg}::StringPtr::Static("{extension_name}")
                }}

                fn default_extension_value(&self) -> {pkg}::Value {{
                    use {pkg}::SingularValue;
                    {default_value}
                }}
            }}            

            pub trait {trait_name} {{
                // TODO: Add has_ accessor and clear_accessors

                fn {field_name}(&self) -> {pkg}::WireResult<ExtensionRef<{field_type}>>;
                fn {field_name}_mut(&mut self) -> {pkg}::WireResult<&mut {field_type}>;
            }}

            impl {trait_name} for {extendee_typename} {{
                fn {field_name}(&self) -> {pkg}::WireResult<ExtensionRef<{field_type}>> {{
                    use {pkg}::ExtensionRef;
                    use common::any::AsAny;;
                    
                    let v = self.extensions()
                        .ok_or({pkg}::WireError::BadDescriptor)?
                        .get_dynamic(&{tag_name} {{}})?;

                    Ok(match v {{
                        ExtensionRef::Pointer(v) => match v {{
                            {value_case} => {{
                                ExtensionRef::Pointer({value_get_ref})
                            }}
                            _ => return Err({pkg}::WireError::BadDescriptor)
                        }}
                        ExtensionRef::Owned(v) => match v {{
                            {value_case} => {{
                                {value_get_owned}
                            }}
                            _ => return Err({pkg}::WireError::BadDescriptor)
                        }}
                        // Should never be returned by get_dynamic().
                        ExtensionRef::Boxed(v) => todo!()
                    }})
                }}

                fn {field_name}_mut(&mut self) -> {pkg}::WireResult<&mut {field_type}> {{
                    use {pkg}::{{SingularValue, RepeatedValues, Value}};
                    use common::any::AsAny;;

                    let v = self.extensions_mut()
                        .ok_or({pkg}::WireError::BadDescriptor)?
                        .get_dynamic_mut(&{tag_name} {{}})?;

                    Ok(match v {{
                        {value_case} => {{
                            {value_get_mut}
                        }}
                        _ => return Err({pkg}::WireError::BadDescriptor)
                    }})
                }}
            }} 
        "#,
            extendee_typename = extendee_typename,
            trait_name = trait_name,
            field_type = field_type,
            tag_name = tag_name,
            pkg = self.options.runtime_package,
            extension_number = extension.extension_number(),
            extension_name = extension.extension_name().deref(),
            default_value = default_value,
            value_case = value_case,
            value_get_mut = value_get_mut,
            value_get_ref = value_get_ref,
            value_get_owned = value_get_owned
        ))
    }

    fn compile_topleveldef(&mut self, def: TypeDescriptor) -> Result<String> {
        Ok(match def {
            TypeDescriptor::Message(m) => self.compile_message(&m)?,
            TypeDescriptor::Enum(e) => self.compile_enum(&e)?,
            TypeDescriptor::Service(s) => self.compile_service(&s)?,
            TypeDescriptor::Extend(e) => self.compile_extension(&e)?,
        })
    }
}
