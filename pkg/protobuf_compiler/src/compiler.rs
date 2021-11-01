// Code for taking a parsed .proto file descriptor and performing code
// generation into Rust code.

use std::collections::HashSet;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use common::errors::*;
use common::line_builder::*;
use protobuf_core::tokenizer::serialize_str_lit;
use protobuf_core::FieldNumber;
use protobuf_core::Message;

use crate::spec::*;

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

#[derive(Clone)]
pub struct CompilerOptions {
    pub runtime_package: String,
    pub rpc_package: String,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            runtime_package: "::protobuf".into(),
            rpc_package: "::rpc".into(),
        }
    }
}

// Roughly similar to the descriptor database in the regular protobuf library
// Stores all parsed .proto files currently loaded
struct DescriptorDatabase {
    base_dir: String,
}

enum ResolvedTypeDesc<'a> {
    Message(&'a MessageDescriptor),
    Enum(&'a Enum),
}

struct ResolvedType<'a> {
    // Name of the type in the currently being compiled source file.
    typename: String,
    descriptor: ResolvedTypeDesc<'a>,
}

struct ImportedProto {
    proto: Proto,
    package_path: String,
}

pub struct Compiler<'a> {
    // The current top level code string that we are building.
    outer: String,

    // Top level proto file descriptor that is being compiled
    proto: &'a Proto,

    imported_protos: Vec<ImportedProto>,

    options: CompilerOptions, /* TODO: Will also need a DescriptorDatabase to look up items in
                               * other files runtime_package:
                               * String */
}

/*
    TODO:
    Things to validate about a proto file
    - All definitions at the same level have distinct names
    - Enum fields and message fields have have distinct names
    - All message fields have distinct numbers
*/

type TypePath<'a> = &'a [&'a str];

trait Resolvable {
    fn resolve(&self, path: TypePath) -> Option<ResolvedType>;
}

impl Resolvable for MessageDescriptor {
    fn resolve(&self, path: TypePath) -> Option<ResolvedType> {
        if path.len() >= 1 && path[0] == &self.name {
            if path.len() == 1 {
                Some(ResolvedType {
                    typename: self.name.clone(),
                    descriptor: ResolvedTypeDesc::Message(self),
                })
            } else {
                // Look for a type inside of the message with the current name.
                for item in &self.body {
                    let inner = match item {
                        MessageItem::Enum(e) => e.resolve(&path[1..]),
                        MessageItem::Message(m) => m.resolve(&path[1..]),
                        _ => None,
                    };

                    // If we found it, prepend the name of the message.
                    if let Some(mut t) = inner {
                        t.typename = format!("{}_{}", self.name, t.typename);
                        return Some(t);
                    }
                }

                None
            }
        } else {
            None
        }
    }
}

impl Resolvable for Enum {
    fn resolve(&self, path: &[&str]) -> Option<ResolvedType> {
        if path.len() == 1 && path[0] == &self.name {
            Some(ResolvedType {
                typename: self.name.clone(),
                descriptor: ResolvedTypeDesc::Enum(self),
            })
        } else {
            None
        }
    }
}

impl Resolvable for Proto {
    fn resolve(&self, path: &[&str]) -> Option<ResolvedType> {
        let mut package = self.package.split('.').collect::<Vec<_>>();
        if package.len() == 1 && package[0].len() == 0 {
            package.pop();
        }

        // In order to be a type in this proto file, the start of the path must be the
        // package name of this proto file.
        if path.len() < package.len() || &package != &path[0..package.len()] {
            return None;
        }

        let relative_path = &path[package.len()..];

        for def in &self.definitions {
            let inner = match def {
                TopLevelDef::Enum(e) => e.resolve(&relative_path),
                TopLevelDef::Message(m) => m.resolve(&relative_path),
                _ => None,
            };

            if inner.is_some() {
                return inner;
            }
        }

        None
    }
}

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

impl Compiler<'_> {
    pub fn compile(
        desc: &Proto,
        path: &Path,
        current_package: &str,
        options: &CompilerOptions,
    ) -> Result<String> {
        let mut c = Compiler {
            outer: String::new(),
            proto: desc,
            options: options.clone(),
            imported_protos: vec![],
        };

        c.outer += "// AUTOGENERATED BY PROTOBUF COMPILER\n\n";
        c.outer += "use std::sync::Arc;\n\n";
        c.outer += "use common::errors::*;\n";
        c.outer += "use common::const_default::ConstDefault;\n";
        write!(c.outer, "use {}::*;\n", c.options.runtime_package).unwrap();
        write!(c.outer, "use {}::wire::*;\n", c.options.runtime_package).unwrap();
        // write!(c.outer, "use {}::service::*;\n", c.options.runtime_package).unwrap();
        write!(
            c.outer,
            "use {}::reflection::*;\n",
            c.options.runtime_package
        )
        .unwrap();

        // TODO: Have an in-process cache for reading imported descriptors from disk.
        for import in &desc.imports {
            let relative_path = std::path::Path::new(&import.path);

            let mut package_path = String::from("::");

            let components = relative_path.components().collect::<Vec<_>>();
            if components.len() <= 3 {
                return Err(format_err!(
                    "Unsupported path format in import: {}",
                    import.path
                ));
            }

            assert!(components.len() > 3);
            for i in 0..(components.len() - 1) {
                if i == 0 {
                    if components[0].as_os_str() != "pkg"
                        && components[0].as_os_str() != "third_party"
                    {
                        return Err(err_msg("Expected to be the pkg|third_party dir"));
                    }

                    continue;
                }
                if i == 1 {
                    if components[i].as_os_str() == current_package {
                        package_path = "crate::".to_string();
                        continue;
                    }

                    // If we are in the same package, we need to use 'crate'
                }
                if i == 2 {
                    // TODO: Check that it is 'src'
                    continue;
                }

                let mut s = components[i].as_os_str().to_str().unwrap();
                if s == "type" {
                    s = "r#type";
                }

                package_path.push_str(s);
                package_path.push_str("::");
            }

            if relative_path.extension().unwrap_or_default() != "proto" {
                return Err(err_msg(
                    "Expected a .proto extension for imported proto files",
                ));
            }

            // TODO: Dedup with above.
            let mut s = relative_path.file_stem().unwrap().to_str().unwrap();
            if s == "type" {
                s = "r#type";
            }

            package_path.push_str(s);
            // use_statement.push_str(";\n");
            // c.outer += &use_statement;

            let full_path = common::project_dir().join(relative_path);
            // let

            // TODO: Should have a register of parsed files if we are doing it in the same
            // process.
            let imported_file = match std::fs::read_to_string(&full_path) {
                Ok(data) => data,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        return Err(format_err!(
                            "Imported proto file not found: {}",
                            import.path
                        ));
                    }

                    return Err(e.into());
                }
            };

            let imported_proto_value = crate::syntax::parse_proto(&imported_file)
                .map_err(|e| format_err!("Failed while parsing {}: {:?}", import.path, e))?;

            c.imported_protos.push(ImportedProto {
                proto: imported_proto_value,
                package_path,
            });
        }

        c.outer.push_str("\n");

        // Add the file descriptor
        {
            let mut p = c.proto.to_proto();
            p.set_name(path.strip_prefix(common::project_dir())?.to_str().unwrap());

            let proto = rust_bytestring(&p.serialize()?);
            let mut deps = vec![];
            for import in &c.imported_protos {
                deps.push(format!("&{}::FILE_DESCRIPTOR", import.package_path));
            }

            c.outer.push_str(&format!(
                "
            pub static FILE_DESCRIPTOR: {runtime_pkg}::StaticFileDescriptor = {runtime_pkg}::StaticFileDescriptor {{
                proto: {proto},
                dependencies: &[{deps}]
            }};
            ",
            runtime_pkg = c.options.runtime_package,
                proto = proto,
                deps = deps.join(", ")
            ));
        }

        let path: TypePath = &[];

        for def in &desc.definitions {
            let s = c.compile_topleveldef(def, &path)?;
            c.outer.push_str(&s);
            c.outer.push('\n');
        }

        Ok(c.outer)
    }

    fn resolve(&self, name_str: &str, path: TypePath) -> Option<ResolvedType> {
        let name = name_str.split('.').collect::<Vec<_>>();
        if name[0] == "" {
            panic!("Absolute paths currently not supported");
        }

        let mut package_path = self.proto.package.split('.').collect::<Vec<_>>();
        if package_path.len() == 1 && package_path[0].len() == 0 {
            package_path.pop();
        }

        package_path.extend(path);

        let mut current_prefix = &package_path[..];
        loop {
            let mut fullname = current_prefix.to_vec();
            fullname.extend_from_slice(&name);

            let t = self.proto.resolve(&fullname);
            if t.is_some() {
                return t;
            }

            // TODO: Eventually we need to check the package names.
            for imported_proto in &self.imported_protos {
                let t = imported_proto.proto.resolve(&fullname);
                if let Some(mut t) = t {
                    t.typename = format!("{}::{}", imported_proto.package_path, t.typename);
                    return Some(t);
                }
            }

            if current_prefix.len() == 0 {
                break;
            }

            // For path 'x.y.z', try 'x.y' next time.
            current_prefix = &current_prefix[0..(current_prefix.len() - 1)];
        }

        None
    }

    fn compile_enum_field(&self, f: &EnumField) -> String {
        format!("\t{} = {},", f.name, f.num)
    }

    fn compile_enum(&self, e: &Enum, path: TypePath) -> String {
        if self.proto.syntax == Syntax::Proto3 {
            let mut has_default = false;
            for i in &e.body {
                if let EnumBodyItem::Field(f) = i {
                    if f.num == 0 {
                        has_default = true;
                        break;
                    }
                }
            }

            if !has_default {
                // TODO: Return an error.
            }
        }

        let mut lines = LineBuilder::new();

        // Because we can't put an enum inside of a struct in Rust, we instead
        // create a top level enum.
        // TODO: Need to consistently escape _'s in the original name.
        let fullname = {
            let mut inner_path = path.to_owned();
            inner_path.push(&e.name);
            inner_path.join("_")
        };

        lines.add("#[derive(Clone, Copy, PartialEq, Eq, Debug)]");
        lines.add(format!("pub enum {} {{", fullname));
        for i in &e.body {
            match i {
                EnumBodyItem::Option(_) => {}
                EnumBodyItem::Field(f) => {
                    lines.add(self.compile_enum_field(f));
                }
            }
        }
        lines.add("}");
        lines.nl();

        // TODO: RE-use this above with the proto3 check.
        let mut default_option = None;
        for i in &e.body {
            match i {
                EnumBodyItem::Option(_) => {}
                EnumBodyItem::Field(f) => {
                    if f.num == 0 || (self.proto.syntax == Syntax::Proto2) {
                        default_option = Some(&f.name);
                        break;
                    }
                }
            }
        }

        // TODO: Throw an error if the enum is used in a non-optional field when it
        // doesn't have a default value.
        if let Some(option_name) = default_option {
            lines.add(format!("impl std::default::Default for {} {{", fullname));
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
            "impl {}::Enum for {} {{",
            self.options.runtime_package, fullname
        ));
        lines.indented(|lines| {
            // TODO: Just make from_usize an Option<>
            lines.add(format!(
                "fn parse(v: {}::EnumValue) -> Result<Self> {{",
                self.options.runtime_package
            ));
            lines.indented(|lines| {
                lines.add("Ok(match v {");
                for i in &e.body {
                    match i {
                        EnumBodyItem::Option(_) => {}
                        EnumBodyItem::Field(f) => {
                            lines.add(format!("\t{} => {}::{},", f.num, fullname, f.name));
                        }
                    }
                }

                lines.add("\t_ => { return Err(err_msg(\"Unknown enum value\")); }");

                lines.add("})");
            });
            lines.add("}");
            lines.nl();

            // fn parse_name(&mut self, name: &str) -> Result<()>;
            lines.add("fn parse_name(s: &str) -> Result<Self> {");
            lines.add("\tOk(match s {");
            for i in &e.body {
                match i {
                    EnumBodyItem::Option(_) => {}
                    EnumBodyItem::Field(f) => {
                        lines.add(format!("\t\t\"{}\" => Self::{},", f.name, f.name));
                    }
                }
            }
            lines.add("_ => { return Err(format_err!(\"Unknown enum name: {}\", s)); }");
            lines.add("})");
            lines.add("}");

            lines.add("fn name(&self) -> &'static str {");
            lines.add("\tmatch self {");
            for i in &e.body {
                match i {
                    EnumBodyItem::Option(_) => {}
                    EnumBodyItem::Field(f) => {
                        lines.add(format!("\t\tSelf::{} => \"{}\",", f.name, f.name));
                    }
                }
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
                "fn assign(&mut self, v: {}::EnumValue) -> Result<()> {{",
                self.options.runtime_package
            ));
            lines.add("\t*self = Self::parse(v)?; Ok(())");
            lines.add("}");
            lines.nl();

            lines.add("fn assign_name(&mut self, s: &str) -> Result<()> {");
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
            lines.add(format!("fn reflect(&self) -> Option<{}::reflection::Reflection> {{ Some({}::reflection::Reflection::Enum(self)) }}",
                      self.options.runtime_package, self.options.runtime_package));
            lines.add(format!("fn reflect_mut(&mut self) -> {}::reflection::ReflectionMut {{ {}::reflection::ReflectionMut::Enum(self) }}",
                      self.options.runtime_package, self.options.runtime_package));
        });
        lines.add("}");

        lines.to_string()
    }

    fn compile_field_type(&self, typ: &FieldType, path: TypePath) -> String {
        String::from(match typ {
            FieldType::Double => "f64",
            FieldType::Float => "f32",
            FieldType::Int32 => "i32",
            FieldType::Int64 => "i64",
            FieldType::Uint32 => "u32",
            FieldType::Uint64 => "u64",
            FieldType::Sint32 => "i32",
            FieldType::Sint64 => "i64",
            FieldType::Fixed32 => "u32",
            FieldType::Fixed64 => "u64",
            FieldType::Sfixed32 => "i32",
            FieldType::Sfixed64 => "i64",
            FieldType::Bool => "bool",
            FieldType::String => "String",
            FieldType::Bytes => "BytesField",
            // TODO: This must resolve the right module (and do any nesting
            // conversions needed)
            // ^ There
            FieldType::Named(s) => {
                return self
                    .resolve(&s, path)
                    .expect(&format!("Failed to resolve field type: {}", s))
                    .typename;
            }
        })
    }

    fn field_name<'a>(&self, field: &'a Field) -> &'a str {
        if field.name == "type" {
            "typ"
        } else {
            &field.name
        }
    }

    fn field_name_inner(name: &str) -> &str {
        if name == "type" {
            "typ"
        } else {
            name
        }
    }

    /// Checks if a type is a 'primitive'.
    ///
    /// A primitive is defined mainly as anything but a nested message type.
    /// In proto3, the presence of primitive fields is undefined.
    fn is_primitive(&self, typ: &FieldType, path: TypePath) -> bool {
        if let FieldType::Named(name) = typ {
            let resolved = self
                .resolve(name, path)
                .expect(&format!("Failed to resolve type: {}", name));
            match resolved.descriptor {
                ResolvedTypeDesc::Enum(_) => true,
                ResolvedTypeDesc::Message(m) => {
                    for item in &m.body {
                        if let MessageItem::Option(o) = item {
                            if o.name == "typed_num" {
                                // TODO: Must check if a boolean and no duplicate options and that
                                // the boolean value is true
                                return true;
                            }
                        }
                    }

                    false
                }
            }
        } else {
            true
        }
    }

    fn is_copyable(&self, typ: &FieldType, path: TypePath) -> bool {
        match typ {
            FieldType::Named(name) => {
                let resolved = self
                    .resolve(name, path)
                    .expect(&format!("Failed to resolve type: {}", name));
                match resolved.descriptor {
                    ResolvedTypeDesc::Enum(_) => true,
                    ResolvedTypeDesc::Message(m) => {
                        for item in &m.body {
                            if let MessageItem::Option(o) = item {
                                if o.name == "typed_num" {
                                    // TODO: Must check if a boolean and no duplicate options and
                                    // that the boolean value is true
                                    return true;
                                }
                            }
                        }

                        false
                    }
                }
            }
            FieldType::String => false,
            FieldType::Bytes => false,
            _ => true,
        }
    }

    fn is_message(&self, typ: &FieldType, path: TypePath) -> bool {
        match typ {
            FieldType::Named(name) => {
                let resolved = self
                    .resolve(name, path)
                    .expect(&format!("Failed to resolve type: {}", name));
                match resolved.descriptor {
                    ResolvedTypeDesc::Enum(_) => false,
                    ResolvedTypeDesc::Message(_) => true,
                }
            }
            _ => false,
        }
    }

    fn is_unordered_set(&self, field: &Field) -> bool {
        if field.label != Label::Repeated {
            return false;
        }

        for opt in &field.unknown_options {
            if opt.name == "unordered_set" {
                return true;
            }
        }

        false
    }

    fn compile_field(&self, field: &Field, path: TypePath) -> String {
        let mut s = String::new();
        s += self.field_name(field);
        s += ": ";

        let typ = self.compile_field_type(&field.typ, path);

        let is_repeated = field.label == Label::Repeated;

        if self.is_unordered_set(field) {
            s += &format!("{}::SetField<{}>", self.options.runtime_package, typ);
        } else if is_repeated {
            s += &format!("Vec<{}>", typ);
        } else {
            if self.is_primitive(&field.typ, path) && self.proto.syntax == Syntax::Proto3 {
                s += &typ;
            } else {
                // We must box raw messages if they may cause cycles.
                // TODO: Do the same thing for groups.
                if self.is_message(&field.typ, path) {
                    s += &format!("Option<MessagePtr<{}>>", typ);
                } else {
                    s += &format!("Option<{}>", typ);
                }
            }
        }

        s += ",";
        s
    }

    fn oneof_typename(&self, oneof: &OneOf, path: TypePath) -> String {
        path.join("_") + &common::snake_to_camel_case(&oneof.name) + "Case"
    }

    fn compile_oneof(&mut self, oneof: &OneOf, path: TypePath) -> CompiledOneOf {
        let mut lines = LineBuilder::new();

        let typename = self.oneof_typename(oneof, path);

        lines.add("#[derive(Debug, Clone)]");
        lines.add(format!("pub enum {} {{", typename));
        lines.add("\tUnknown,");
        for field in &oneof.fields {
            let typ = self.compile_field_type(&field.typ, path);
            lines.add(format!(
                "\t{}({}),",
                common::snake_to_camel_case(&field.name),
                typ
            ));
        }
        lines.add("}");
        lines.nl();

        lines.add(format!("impl Default for {} {{", typename));
        lines.add("\tfn default() -> Self {");
        lines.add(format!("\t\t{}::Unknown", typename));
        lines.add("\t}");
        lines.add("}");
        lines.nl();

        lines.add(format!(
            "impl common::const_default::ConstDefault for {} {{",
            typename
        ));
        lines.add(format!(
            "\tconst DEFAULT: {} = {}::Unknown;",
            typename, typename
        ));
        lines.add("}");
        lines.nl();

        CompiledOneOf {
            typename,
            source: lines.to_string(),
        }
    }

    /// Compiles a single
    fn compile_message_item(
        &mut self,
        item: &MessageItem,
        path: TypePath,
    ) -> Result<Option<String>> {
        Ok(match item {
            MessageItem::Enum(e) => {
                self.outer.push_str(&self.compile_enum(e, path));
                None
            }
            MessageItem::Message(m) => {
                let data = self.compile_message(m, path)?;
                self.outer.push_str(&data);
                None
            }
            // MessageItem::Message(msg) => Self::compile_message(outer, msg),
            MessageItem::Field(f) => Some(self.compile_field(f, path)),

            MessageItem::MapField(f) => {
                let mut s = String::new();
                s += &f.name; // TODO: Handle 'type' -> 'typ'
                s += &format!(": {}::MapField<", &self.options.runtime_package);
                s += &self.compile_field_type(&f.key_type, path);
                s += ", ";
                s += &self.compile_field_type(&f.value_type, path);
                s += ">,";
                Some(s)
            }

            MessageItem::OneOf(oneof) => {
                let compiled = self.compile_oneof(oneof, path);
                self.outer.push_str(&compiled.source);
                Some(format!(
                    "{}: {},",
                    Self::field_name_inner(&oneof.name),
                    compiled.typename
                ))
            }

            MessageItem::Reserved(_) => None,

            _ => None,
        })
    }

    fn compile_constant(&self, typ: &FieldType, constant: &Constant, path: TypePath) -> String {
        match constant {
            Constant::Identifier(v) => {
                let enum_name = self.compile_field_type(typ, path);
                format!("{}::{}", enum_name, v)
            }
            Constant::Integer(v) => v.to_string(),
            Constant::Float(v) => v.to_string(),
            Constant::String(v) => {
                let mut out = String::new();
                serialize_str_lit(v.as_bytes(), &mut out);
                out
            }
            Constant::Bool(v) => if *v { "true" } else { "false" }.to_string(),
        }
    }

    fn compile_field_accessors(
        &self,
        field: &Field,
        path: TypePath,
        oneof: Option<&OneOf>,
    ) -> String {
        let mut lines = LineBuilder::new();

        let name = self.field_name(field);

        // TODO: Verify the given label is allowed in the current syntax
        // version

        // TOOD: verify that the label is handled appropriately for oneof fields.

        let is_repeated = field.label == Label::Repeated;
        let typ = self.compile_field_type(&field.typ, &path);

        let is_primitive = self.is_primitive(&field.typ, &path);
        let is_copyable = self.is_copyable(&field.typ, &path);
        let is_message = self.is_message(&field.typ, &path);

        // NOTE: We

        let oneof_option = !(is_primitive && self.proto.syntax == Syntax::Proto3);

        // TODO: Messages should always have options?
        let use_option =
            !((is_primitive && self.proto.syntax == Syntax::Proto3) || oneof.is_some());

        // field()
        if self.is_unordered_set(field) {
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
            lines.add(format!(
                r#"
                pub fn {name}(&self) -> &[{typ}] {{
                    &self.{name}
                }}
            
                pub fn {name}_mut(&mut self) -> &mut [{typ}] {{
                    &mut self.{name}
                }}
            "#,
                name = name,
                typ = typ
            ));
        } else {
            let modifier = if is_copyable { "" } else { "&" };

            let rettype = {
                match &field.typ {
                    FieldType::String => "str",
                    FieldType::Bytes => "[u8]",
                    _ => &typ,
                }
            };

            // TODO: Need to read the 'default' property

            let explicit_default = field.unknown_options.iter().find(|o| o.name == "default");

            let default_value = {
                if let Some(opt) = &explicit_default {
                    self.compile_constant(&field.typ, &opt.value, path)
                } else if rettype == "str" {
                    "\"\"".to_string()
                } else if rettype == "[u8]" {
                    "&[]".to_string()
                } else if is_message && !is_copyable {
                    format!("{}::static_default_value()", typ)
                } else {
                    // For now it's a const,
                    format!("{}{}::DEFAULT", modifier, typ)
                }
            };

            // NOTE: For primitives, it is sufficient to copy it.
            lines.add(format!(
                "\tpub fn {}(&self) -> {}{} {{",
                name, modifier, rettype
            ));
            if use_option {
                if is_copyable {
                    if explicit_default.is_some() {
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
                let oneof_typename = self.oneof_typename(oneof, path);
                let oneof_fieldname = Self::field_name_inner(&oneof.name);
                let oneof_case = common::snake_to_camel_case(&field.name);

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

        if self.is_unordered_set(field) {
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
            lines.add(format!(
                "\tpub fn add_{}(&mut self, v: {}) -> &mut {} {{",
                name, typ, typ
            ));
            lines.add(format!(
                "\t\tself.{}.push(v); self.{}.last_mut().unwrap()",
                name, name
            ));
            lines.add("\t}");

        // NOTE: We do not define 'fn add_field() -> &mut T'
        } else {
            if
            /* is_primitive */
            true {
                // set_field(v: T)
                lines.add(format!(
                    "\tpub fn set_{}<V: ::std::convert::Into<{}>>(&mut self, v: V) {{",
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
                        let oneof_typename = self.oneof_typename(oneof, path);
                        let oneof_fieldname = Self::field_name_inner(&oneof.name);
                        let oneof_case = common::snake_to_camel_case(&field.name);

                        lines.add(format!(
                            "\t\tself.{} = {}::{}(v);",
                            oneof_fieldname, oneof_typename, oneof_case
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
                        " self.{}.get_or_insert_with(|| {}::default()) }}",
                        name, typ
                    ));
                }
            } else {
                if let Some(oneof) = oneof.clone() {
                    let oneof_typename = self.oneof_typename(oneof, path);
                    let oneof_fieldname = Self::field_name_inner(&oneof.name);
                    let oneof_case = common::snake_to_camel_case(&field.name);

                    // Step 1: Ensure that the enum has the right case. Else give it a default
                    // value.
                    lines.add(format!("\t\tif let {}::{}(_) = &self.{} {{ }} else {{ self.{} = {}::{}({}::DEFAULT); }}",
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

        lines.to_string()
    }

    fn compile_message(&mut self, msg: &MessageDescriptor, path: TypePath) -> Result<String> {
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

        let mut inner_path = Vec::from(path);
        inner_path.push(&msg.name);

        // TOOD: Must validate that the typed num field contains only one field which is
        // an integer type.
        let mut is_typed_num = false;
        // let mut can_be_typed_num = true;

        let mut used_nums: HashSet<FieldNumber> = HashSet::new();
        for item in &msg.body {
            match item {
                MessageItem::Field(field) => {
                    if !used_nums.insert(field.num) {
                        panic!("Duplicate field number: {}", field.num);
                    }
                    if self.proto.syntax == Syntax::Proto3 {
                        if field.label != Label::None && field.label != Label::Repeated {
                            panic!("Invalid field label in proto3");
                        }
                    } else {
                        if field.label == Label::None {
                            panic!("Missing field label in proto2 field");
                        }
                    }
                }
                MessageItem::OneOf(oneof) => {
                    for field in &oneof.fields {
                        if !used_nums.insert(field.num) {
                            panic!("Duplicate field number: {}", field.num);
                        }
                        if field.label != Label::None {
                            panic!(
                                "Labels not allowed for a 'oneof' field: {} => {:?}.",
                                field.name, field.label
                            );
                        }
                    }
                }
                MessageItem::MapField(map_field) => {
                    if !used_nums.insert(map_field.num) {
                        panic!("Duplicate field number: {}", map_field.num);
                    }
                }
                MessageItem::Option(option) => {
                    if option.name == "typed_num" {
                        is_typed_num = match option.value {
                            Constant::Bool(v) => v,
                            _ => {
                                return Err(err_msg(
                                    "Expected typed_num option to have boolean value",
                                ));
                            }
                        };
                    }
                }
                _ => {}
            }
        }
        // TODO: Validate reserved field numbers/names.

        // TOOD: Debug with the enum code
        let fullname: String = inner_path.join("_");

        let mut lines = LineBuilder::new();
        // NOTE: We intentionally don't derive PartialEq always as it can be error prone
        // (especially with Option<> types). TODO: Use the debug_string to
        // implement debug.
        lines.add("#[derive(Clone, Default, ConstDefault)]");
        lines.add(format!("pub struct {} {{", fullname));
        lines.indented(|lines| -> Result<()> {
            for i in &msg.body {
                if let Some(field) = self.compile_message_item(&i, &inner_path)? {
                    lines.add(field);
                }
            }

            Ok(())
        })?;
        lines.add("}");
        lines.nl();

        lines.add(format!(
            "
            impl ::std::fmt::Debug for {name} {{
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {{
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

            if let FieldType::Named(_) = field.typ {
                return Err(err_msg("typed_num message field must be a primitive type"));
            }

            let field_type = self.compile_field_type(&field.typ, &[]);

            lines.add(format!(
                r#"
                impl Copy for {msg_name} {{}}

                impl ::std::ops::Add<Self> for {msg_name} {{
                    type Output = Self;
                    
                    fn add(self, other: Self) -> Self {{
                        let mut sum = self.clone();
                        *sum.{field_name}_mut() += other.{field_name}();
                        sum 
                    }}
                }}

                impl ::std::ops::Add<{field_type}> for {msg_name} {{
                    type Output = Self;
                    
                    fn add(self, other: {field_type}) -> Self {{
                        let mut sum = self.clone();
                        *sum.{field_name}_mut() += other;
                        sum
                    }}
                }}

                impl ::std::ops::Sub<{field_type}> for {msg_name} {{
                    type Output = Self;
                    
                    fn sub(self, other: {field_type}) -> Self {{
                        let mut result = self.clone();
                        *result.{field_name}_mut() -= other;
                        result
                    }}
                }}

                impl ::std::cmp::PartialEq for {msg_name} {{
                    fn eq(&self, other: &Self) -> bool {{
                        self.{field_name}() == other.{field_name}()
                    }}
                }}

                impl Eq for {msg_name} {{}}

                impl ::std::cmp::PartialOrd for {msg_name} {{
                    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {{
                        Some(self.cmp(other))
                    }}
                }}
                
                impl Ord for {msg_name} {{
                    fn cmp(&self, other: &Self) -> ::std::cmp::Ordering {{
                        self.{field_name}().cmp(&other.{field_name}())
                    }}
                }}
                
                impl ::std::convert::From<{field_type}> for {msg_name} {{
                    fn from(v: {field_type}) -> Self {{
                        let mut inst = Self::default();
                        inst.set_{field_name}(v);
                        inst
                    }}
                }}

                impl ::std::hash::Hash for {msg_name} {{
                    fn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {{
                        self.{field_name}().hash(state);
                    }}
                }}

            "#,
                msg_name = msg.name,
                field_type = field_type,
                field_name = field.name
            ));
        }

        lines.add(format!("impl {} {{", fullname));

        lines.add("\tpub fn static_default_value() -> &'static Self {");
        lines.add(format!(
            "\t\tstatic VALUE: {} = {}::DEFAULT;",
            fullname, fullname
        ));
        lines.add("\t\t&VALUE");
        lines.add("\t}");
        lines.nl();

        for item in &msg.body {
            match item {
                MessageItem::OneOf(oneof) => {
                    let oneof_typename = self.oneof_typename(oneof, &inner_path);
                    lines.add(format!(
                        "\tpub fn {}_case(&self) -> &{} {{ &self.{} }}",
                        oneof.name,
                        oneof_typename,
                        Self::field_name_inner(&oneof.name)
                    ));

                    for field in &oneof.fields {
                        lines.add(self.compile_field_accessors(field, &inner_path, Some(oneof)));
                    }

                    // Should also add fields to assign to each of the items
                }
                MessageItem::Field(field) => {
                    lines.add(self.compile_field_accessors(field, &inner_path, None));
                }
                // TODO: Add map field accessors.
                _ => {}
            }
        }

        lines.add("}");
        lines.nl();

        lines.add(format!(
            "impl {}::Message for {} {{",
            self.options.runtime_package, fullname
        ));

        // "type.googleapis.com/"
        let mut type_url_parts = vec![];
        if !self.proto.package.is_empty() {
            type_url_parts.push(self.proto.package.as_str());
        }
        type_url_parts.extend_from_slice(&inner_path);

        lines.add(format!(
            r#"
            fn type_url(&self) -> &'static str {{
                "type.googleapis.com/{type_url}"
            }}

            fn file_descriptor() -> &'static {runtime_pkg}::StaticFileDescriptor {{
                &FILE_DESCRIPTOR
            }}
        
        "#,
            type_url = type_url_parts.join("."),
            runtime_pkg = self.options.runtime_package
        ));

        lines.add(
            r#"
            fn parse(data: &[u8]) -> Result<Self> {
                let mut msg = Self::default();
                msg.parse_merge(data)?;
                Ok(msg)
            } 
        "#,
        );

        lines.add("\tfn parse_merge(&mut self, data: &[u8]) -> Result<()> {");
        lines.add("\t\tfor f in WireFieldIter::new(data) {");
        lines.add("\t\t\tlet f = f?;");
        lines.add("\t\t\tmatch f.field_number {");

        for field in msg.fields() {
            let name = self.field_name(field);
            let is_repeated = field.label == Label::Repeated;

            let use_option = !(self.is_primitive(&field.typ, &inner_path)
                && self.proto.syntax == Syntax::Proto3);

            let is_message = self.is_message(&field.typ, &inner_path);

            let typeclass = match &field.typ {
                FieldType::Named(n) => {
                    // TODO: Call compile_field_type
                    let typ = self
                        .resolve(&n, &inner_path)
                        .expect(&format!("Failed to resolve type type: {}", n));

                    match &typ.descriptor {
                        ResolvedTypeDesc::Enum(_) => "enum",
                        ResolvedTypeDesc::Message(_) => "message",
                    }
                }
                _ => field.typ.as_str(),
            };

            let mut p = String::new();
            if self.is_unordered_set(field) {
                p += &format!("self.{}.insert(f.parse_{}()?);", name, typeclass)
            } else if is_repeated {
                p += &format!("self.{}.push(f.parse_{}()?);", name, typeclass);
            } else {
                if use_option {
                    if is_message {
                        // TODO: also need to do this which not using optional but we are in
                        // proto2 required mode?
                        p += &format!(
                            "self.{} = Some(MessagePtr::new(f.parse_{}()?))",
                            name, typeclass
                        );
                    } else {
                        p += &format!("self.{} = Some(f.parse_{}()?)", name, typeclass);
                    }
                } else {
                    p += &format!("self.{} = f.parse_{}()?", name, typeclass);
                }
            }

            lines.add(format!("\t\t\t\t{} => {{ {} }},", field.num, p));
        }

        for item in &msg.body {
            match item {
                MessageItem::OneOf(oneof) => {
                    let oneof_typename = self.oneof_typename(oneof, &inner_path);
                    let oneof_fieldname = Self::field_name_inner(&oneof.name);

                    for field in &oneof.fields {
                        let oneof_case = common::snake_to_camel_case(&field.name);

                        // TODO: Dedup with above
                        let typeclass = match &field.typ {
                            FieldType::Named(n) => {
                                // TODO: Call compile_field_type
                                let typ = self
                                    .resolve(&n, &inner_path)
                                    .expect(&format!("Failed to resolve type type: {}", n));

                                match &typ.descriptor {
                                    ResolvedTypeDesc::Enum(_) => "enum",
                                    ResolvedTypeDesc::Message(_) => "message",
                                }
                            }
                            _ => field.typ.as_str(),
                        };

                        let value = format!("f.parse_{}()?", typeclass);

                        lines.add(format!(
                            "{field_num} => {{ self.{oneof_fieldname} = {oneof_typename}::{oneof_case}({value}); }},",
                            field_num = field.num,
                            oneof_fieldname = oneof_fieldname,
                            oneof_typename = oneof_typename,
                            oneof_case = oneof_case,
                            value = value
                        ));
                    }
                }
                _ => {}
            }
        }

        // TODO: Will need to record this as an unknown field.
        lines.add("\t\t\t\t_ => {}");

        lines.add("\t\t\t}");
        lines.add("\t\t}");
        lines.add("\t\tOk(())");
        lines.add("\t}");

        lines.add("\tfn serialize(&self) -> Result<Vec<u8>> {");
        lines.add("\t\tlet mut data = vec![];");

        // TODO: Sort the serialization by the field numbers so that we get nice cross
        // version compatible formats.

        // TODO: Need to implement packed serialization/deserialization.
        for field in msg.fields() {
            let name = self.field_name(field);
            let is_repeated = field.label == Label::Repeated;
            let is_message = self.is_message(&field.typ, &inner_path);

            // TODO: Dedup with above
            let mut typeclass = match &field.typ {
                FieldType::Named(n) => {
                    let typ = self
                        .resolve(&n, &inner_path)
                        .expect("Failed to resolve type");

                    match &typ.descriptor {
                        ResolvedTypeDesc::Enum(_) => "enum",
                        ResolvedTypeDesc::Message(_) => "message",
                    }
                }
                _ => field.typ.as_str(),
            }
            .to_string();

            let pass_reference = match &field.typ {
                FieldType::String => true,
                FieldType::Named(_) => true,
                FieldType::Bytes => true,
                _ => false,
            };

            let use_option = !(self.is_primitive(&field.typ, &inner_path)
                && self.proto.syntax == Syntax::Proto3)
                && !is_repeated;

            // TODO: Should also check that we aren't using a 'required' label?
            if !use_option && !is_repeated {
                typeclass = format!("sparse_{}", typeclass);
            }

            let given_reference = is_repeated || use_option;

            let reference_str = {
                if pass_reference {
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

            let serialize_line = format!(
                "\t\t\tWireField::serialize_{}({}, {}v, &mut data)?;",
                typeclass, field.num, reference_str
            );

            if is_repeated {
                lines.add(format!("\t\tfor v in self.{}.iter() {{", name));
                lines.add(serialize_line);
                lines.add("\t\t}");
            } else {
                // TODO: For proto3, the requirement is that it is not equal to
                // the default value (and there would be no optional for
                if use_option {
                    lines.add(format!("\t\tif let Some(v) = self.{}.as_ref() {{", name));
                    if is_message {
                        lines.add(format!(
                            "\t\tWireField::serialize_message({}, v.as_ref(), &mut data)?;",
                            field.num
                        ));
                    } else {
                        lines.add(serialize_line);
                    }
                    lines.add("\t\t}");
                } else {
                    // TODO: Should borrow the value when using messages
                    lines.add(format!(
                        "\t\tWireField::serialize_{}({}, {}self.{}, &mut data)?;",
                        typeclass, field.num, reference_str, name,
                    ));
                }

                if field.label == Label::Required {
                    lines.add_inline(" else {");
                    // TODO: Verify the field name doesn't have any quotes in it
                    lines.add(format!(
                        "\treturn Err(err_msg(\"Required field '{}' not set\"));",
                        name
                    ));
                    lines.add("}");
                }
            }
        }

        for item in &msg.body {
            match item {
                MessageItem::OneOf(oneof) => {
                    let oneof_typename = self.oneof_typename(oneof, &inner_path);
                    let oneof_fieldname = Self::field_name_inner(&oneof.name);

                    lines.add(format!("\t\tmatch &self.{} {{", oneof_fieldname));

                    for field in &oneof.fields {
                        let oneof_case = common::snake_to_camel_case(&field.name);

                        // TODO: Dedup with above
                        let typeclass = match &field.typ {
                            FieldType::Named(n) => {
                                let typ = self
                                    .resolve(&n, &inner_path)
                                    .expect("Failed to resolve type");

                                match &typ.descriptor {
                                    ResolvedTypeDesc::Enum(_) => "enum",
                                    ResolvedTypeDesc::Message(_) => "message",
                                }
                            }
                            _ => field.typ.as_str(),
                        }
                        .to_string();

                        // TODO: Deduplicate with above.
                        let pass_reference = match &field.typ {
                            FieldType::String => true,
                            FieldType::Named(_) => true,
                            FieldType::Bytes => true,
                            _ => false,
                        };

                        let reference = if pass_reference { "" } else { "*" };

                        lines.add(format!(
                            "\t\t\t{oneof_typename}::{oneof_case}(v) => {{
                                WireField::serialize_{typeclass}({field_num}, {reference}v, &mut data)?; }}",
                            oneof_typename = oneof_typename,
                            oneof_case = oneof_case,
                            typeclass = typeclass,
                            reference = reference,
                            field_num = field.num
                        ));
                    }

                    lines.add(format!("\t\t\t{}::Unknown => {{}}", oneof_typename));

                    lines.add("\t\t}");
                }
                _ => {}
            }
        }

        lines.add("\t\tOk(data)");
        lines.add("\t}");

        // Implementing merge_from
        // extend_from_slice and assignment

        lines.add("}");
        lines.nl();

        lines.add(format!(
            "impl {}::MessageReflection for {} {{",
            self.options.runtime_package, fullname
        ));

        lines.indented(|lines| {
            lines.add(format!(
                "fn fields(&self) -> &[{}::FieldDescriptorShort] {{",
                self.options.runtime_package
            ));

            let mut all_fields = vec![];
            for item in &msg.body {
                match item {
                    MessageItem::Field(field) => {
                        all_fields.push((field.num, field.name.as_str()));
                    }
                    MessageItem::OneOf(oneof) => {
                        for field in &oneof.fields {
                            all_fields.push((field.num, field.name.as_str()));
                        }
                    }
                    MessageItem::MapField(map) => {
                        all_fields.push((map.num, map.name.as_str()));
                    }
                    _ => {}
                }
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

        lines.indented(|lines| {
            lines.add("fn field_by_number(&self, num: FieldNumber) -> Option<Reflection> {");
            lines.indented(|lines| {
                if msg.body.len() == 0 {
                    lines.add("None");
                    return;
                }

                lines.add("match num {");
                for item in &msg.body {
                    match item {
                        MessageItem::Field(field) => {
                            let name = self.field_name(field);
                            lines.add(format!("\t{} => self.{}.reflect(),", field.num, name));
                        }
                        MessageItem::OneOf(oneof) => {
                            let name = Self::field_name_inner(&oneof.name);

                            // TODO: The issue with this is that we can't distinguish between an
                            // invalid field and an unpopulated
                            for field in &oneof.fields {
                                lines.add(format!("\t{} => {{", field.num));
                                lines.add(format!(
                                    "\t\tif let {}::{}(v) = &self.{} {{",
                                    self.oneof_typename(oneof, &inner_path),
                                    common::snake_to_camel_case(&field.name),
                                    name
                                ));
                                lines.add("\t\t\tv.reflect()");

                                // TODO: Reflect a DEFAULT value
                                lines.add("\t\t} else { None }");
                                lines.add("\t}");
                            }
                        }
                        _ => {}
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
            lines.indented(|lines| {
                if msg.body.len() == 0 {
                    lines.add("None");
                    return;
                }

                lines.add("Some(match num {");
                for item in &msg.body {
                    match item {
                        MessageItem::Field(field) => {
                            let name = self.field_name(field);
                            lines.add(format!("\t{} => self.{}.reflect_mut(),", field.num, name));
                        }
                        MessageItem::OneOf(oneof) => {
                            let name = Self::field_name_inner(&oneof.name);

                            for field in &oneof.fields {
                                lines.add(format!("\t{} => {{", field.num));
                                lines.add(format!(
                                    "\t\tif let {}::{}(v) = &mut self.{} {{}}",
                                    self.oneof_typename(oneof, &inner_path),
                                    common::snake_to_camel_case(&field.name),
                                    name
                                ));
                                lines.add("\t\telse {");

                                let typ = self.compile_field_type(&field.typ, &inner_path);

                                lines.add(format!(
                                    "\t\t\tself.{} = {}::{}({}::default());",
                                    name,
                                    self.oneof_typename(oneof, &inner_path),
                                    common::snake_to_camel_case(&field.name),
                                    typ
                                ));
                                lines.add("\t\t}");
                                lines.nl();

                                lines.add(format!(
                                    "\t\tif let {}::{}(v) = &mut self.{} {{",
                                    self.oneof_typename(oneof, &inner_path),
                                    common::snake_to_camel_case(&field.name),
                                    name
                                ));

                                lines.add("\t\t\tv.reflect_mut()");

                                lines.add("\t\t} else {");
                                lines.add("\t\t\tpanic!();");
                                lines.add("\t\t}");
                                lines.add("\t}");
                            }
                        }
                        _ => {}
                    }
                }
                lines.add("\t_ => { return None; }");
                lines.add("})");
            });
            lines.add("}");
            lines.nl();

            lines.add("fn field_number_by_name(&self, name: &str) -> Option<FieldNumber> {");
            lines.indented(|lines| {
                if msg.body.len() == 0 {
                    lines.add("None");
                    return;
                }

                lines.add("Some(match name {");
                for item in &msg.body {
                    match item {
                        MessageItem::Field(field) => {
                            lines.add(format!("\t\"{}\" => {},", field.name, field.num));
                        }
                        MessageItem::OneOf(oneof) => {
                            for field in &oneof.fields {
                                lines.add(format!("\t\"{}\" => {},", field.name, field.num));
                            }
                        }
                        MessageItem::MapField(map) => {
                            lines.add(format!("\t\"{}\" => {},", map.name, map.num));
                        }
                        _ => {}
                    }
                }

                lines.add("\t_ => { return None; }");
                lines.add("})");
            });
            lines.add("}");
        });

        lines.add("}");

        Ok(lines.to_string())
    }

    // TODO: 'path' will always be empty?
    fn compile_service(&mut self, service: &Service, path: TypePath) -> String {
        //		let modname = common::camel_to_snake_case(&service.name);

        let mut lines = LineBuilder::new();

        // Full name of the service including the package name
        // e.g. google.api.MyService
        let absolute_name: String = {
            let mut parts = vec![];
            if !self.proto.package.is_empty() {
                parts.push(self.proto.package.as_str());
            }
            parts.extend_from_slice(path);
            parts.push(service.name.as_str());

            parts.join(".")
        };

        lines.add(format!(
            r#"
            pub struct {service_name}Stub {{
                channel: Arc<dyn {rpc_package}::Channel>

            }}
        "#,
            service_name = service.name,
            rpc_package = self.options.rpc_package
        ));

        lines.add(format!("impl {}Stub {{", service.name));
        lines.indented(|lines| {
            lines.add(format!("
                pub fn new(channel: Arc<dyn {rpc_package}::Channel>) -> Self {{
                    Self {{ channel }}
                }}
            ", rpc_package = self.options.rpc_package));

            for rpc in service.rpcs() {
                let req_type = self
                    .resolve(&rpc.req_type, path)
                    .expect(&format!("Failed to find {}", rpc.req_type));
                let res_type = self.resolve(&rpc.res_type, path)
                    .expect(&format!("Failed to find {}", rpc.res_type));

                if rpc.req_stream && rpc.res_stream {
                    // Bi-directional streaming

                    lines.add(format!(r#"
                        pub async fn {rpc_name}(&self, request_context: &{rpc_package}::ClientRequestContext)
                            -> ({rpc_package}::ClientStreamingRequest<{req_type}>, {rpc_package}::ClientStreamingResponse<{res_type}>) {{
                            self.channel.call_stream_stream("{service_name}", "{rpc_name}", request_context).await
                        }}"#,
                        rpc_package = self.options.rpc_package,
                        service_name = absolute_name,
                        rpc_name = rpc.name,
                        req_type = req_type.typename,
                        res_type = res_type.typename
                    ));
                } else if rpc.req_stream {
                    // Client streaming

                    lines.add(format!(r#"
                        pub async fn {rpc_name}(&self, request_context: &{rpc_package}::ClientRequestContext)
                            -> {rpc_package}::ClientStreamingCall<{req_type}, {res_type}> {{
                            self.channel.call_stream_unary("{service_name}", "{rpc_name}", request_context).await
                        }}"#,
                        rpc_package = self.options.rpc_package,
                        service_name = absolute_name,
                        rpc_name = rpc.name,
                        req_type = req_type.typename,
                        res_type = res_type.typename
                    ));
                } else if rpc.res_stream {
                    // Server streaming

                    lines.add(format!(r#"
                        pub async fn {rpc_name}(&self, request_context: &{rpc_package}::ClientRequestContext, request_value: &{req_type})
                            -> {rpc_package}::ClientStreamingResponse<{res_type}> {{
                            self.channel.call_unary_stream("{service_name}", "{rpc_name}", request_context, request_value).await
                        }}"#,
                        rpc_package = self.options.rpc_package,
                        service_name = absolute_name,
                        rpc_name = rpc.name,
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
                        rpc_name = rpc.name,
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
            service.name
        ));

        for rpc in service.rpcs() {
            let req_type = self
                .resolve(&rpc.req_type, path)
                .expect(&format!("Failed to find {}", rpc.req_type));
            let res_type = self
                .resolve(&rpc.res_type, path)
                .expect(&format!("Failed to find {}", rpc.res_type));

            // TODO: Must resolve the typename.
            // TODO: I don't need to make the response '&mut' if I am giving a stream.
            lines.add(format!(
                "\tasync fn {rpc_name}(&self, request: {rpc_package}::Server{req_stream}Request<{req_type}>,
                                       response: &mut {rpc_package}::Server{res_stream}Response<{res_type}>) -> Result<()>;",
                rpc_package = self.options.rpc_package,
                rpc_name = rpc.name,
                req_type = req_type.typename,
                req_stream = if rpc.req_stream { "Stream" } else { "" },
                res_type = res_type.typename,
                res_stream = if rpc.res_stream { "Stream" } else { "" },
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
            service_name = service.name
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
            rpc_package = self.options.rpc_package, service_name = service.name));
        lines.indented(|lines| {
            // TODO: Escape the string if needed.
            lines.add(format!(
                "fn service_name(&self) -> &'static str {{ \"{}\" }}",
                absolute_name
            ));

            lines.add(format!(
                "fn file_descriptor(&self) -> &'static {}::StaticFileDescriptor {{ &FILE_DESCRIPTOR }}",
                self.options.runtime_package
            ));

            lines.add("fn method_names(&self) -> &'static [&'static str] {");
            lines.add("\t&[");

            // NOTE: We do not support the support streams feature of proto2
            // TODO: Ensure no streams are defined unless in proto2 mode.
            for rpc in service.rpcs() {
                lines.add_inline(format!("\"{}\", ", rpc.name));
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

                for rpc in service.rpcs() {
                    let req_type = self
                        .resolve(&rpc.req_type, path)
                        .expect(&format!("Failed to find {}", rpc.req_type));
                    let res_type = self.resolve(&rpc.res_type, path)
                        .expect(&format!("Failed to find {}", rpc.res_type));

                    let request_obj = {
                        if rpc.req_stream {
                            format!("request.into::<{}>()", req_type.typename)
                        } else {
                            format!("request.into_unary::<{}>().await?", req_type.typename)
                        }
                    };

                    let response_obj = {
                        if rpc.res_stream {
                            format!("response.into::<{}>()", res_type.typename)
                        } else {
                            format!("response.new_unary::<{}>()", res_type.typename)
                        }
                    };

                    let response_post = {
                        if rpc.res_stream {
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
                        rpc_name = rpc.name,
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

        lines.to_string()
    }

    fn compile_topleveldef(&mut self, def: &TopLevelDef, path: TypePath) -> Result<String> {
        Ok(match def {
            TopLevelDef::Message(m) => self.compile_message(&m, path)?,
            TopLevelDef::Enum(e) => self.compile_enum(e, path),
            TopLevelDef::Service(s) => self.compile_service(s, path),
            _ => String::new(),
        })
    }
}
