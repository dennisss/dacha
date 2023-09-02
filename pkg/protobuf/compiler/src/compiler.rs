// Code for taking a parsed .proto file descriptor and performing code
// generation into Rust code.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Write;

use common::errors::*;
use common::line_builder::*;
use crypto::hasher::Hasher;
use file::LocalPath;
use file::LocalPathBuf;
use protobuf_core::tokenizer::serialize_str_lit;
use protobuf_core::FieldNumber;
use protobuf_core::Message;
use protobuf_dynamic::spec::*;
#[cfg(feature = "descriptors")]
use protobuf_dynamic::DescriptorPool;

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
    /// Paths which will be searched when resolving proto file imports.
    ///
    /// - If a .proto file references 'import "y.proto";', then there must be a
    ///   'x' in this list such that 'x/y.proto' exists.
    /// - The first directory in which a match can be found will be used.
    /// - The relative path in one of these paths is also used as the
    ///   FileDescriptorProto::name.
    pub paths: Vec<LocalPathBuf>,

    /// In not none, protos must be in these directories to be compiled.
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
            paths: vec![],
            runtime_package: "::protobuf".into(),
            rpc_package: "::rpc".into(),
            should_format: false,
            allowlisted_paths: None,
        }
    }
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
    file_path: String,
    proto: Proto,
    package_path: String,
    file_id: String,
}

pub struct Compiler<'a> {
    // The current top level code string that we are building.
    outer: String,

    // Top level proto file descriptor that is being compiled
    proto: &'a Proto,

    imported_protos: Vec<ImportedProto>,


    file_id: String,
}

/*
    TODO:
    Things to validate about a proto file
    - All definitions at the same level have distinct names
    - Enum fields and message fields have have distinct names
    - All message fields have distinct numbers
*/

// TODO: Be consistent about whether or not this is an escaped identifier path
// or now.
type TypePath<'a> = &'a [&'a str];

trait Resolvable {
    fn resolve(&self, path: TypePath) -> Option<ResolvedType>;
}

impl Resolvable for MessageDescriptor {
    fn resolve(&self, path: TypePath) -> Option<ResolvedType> {
        let my_name = escape_rust_identifier(&self.name);

        if path.len() >= 1 && path[0] == &self.name {
            if path.len() == 1 {
                Some(ResolvedType {
                    typename: my_name.to_string(),
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
                        t.typename = format!("{}_{}", my_name, t.typename);
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
                typename: escape_rust_identifier(&self.name).to_string(),
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
        path: &LocalPath,
        current_package_dir: &LocalPath,
        options: &CompilerOptions,
    ) -> Result<(String, String)> {
        let mut c = Compiler {
            outer: String::new(),
            proto: desc,
            options: options.clone(),
            imported_protos: vec![],
            file_id: String::new(),
        };

        let file_name = c.resolve_file_name(path)?;
        let file_id = {
            let id = crypto::sip::SipHasher::default_rounds_with_key_halves(0, 0)
                .finish_with(file_name.as_bytes());
            base_radix::hex_encode(&id).to_ascii_uppercase()
        };
        c.file_id = file_id;

        c.outer += "// AUTOGENERATED BY PROTOBUF COMPILER\n\n";

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
        for import in &desc.imports {
            // TODO: Verify this is in the root_dir (after normalization).
            let relative_path = LocalPath::new(&import.path);

            if relative_path.extension().unwrap_or_default() != "proto" {
                return Err(err_msg(
                    "Expected a .proto extension for imported proto files",
                ));
            }

            let mut full_path = None;
            for base_path in &options.paths {
                let p: LocalPathBuf = base_path.join(relative_path);
                if !std::path::Path::new(p.as_str()).try_exists()? {
                    continue;
                }

                full_path = Some(p);
                break;
            }

            let full_path = full_path
                .ok_or_else(|| format_err!("Imported proto file not found: {}", import.path))?;

            // TODO: Should have a register of parsed files if we are doing it in the same
            // process.
            let imported_file = std::fs::read_to_string(&full_path)?;

            let imported_proto_value = protobuf_dynamic::syntax::parse_proto(&imported_file)
                .map_err(|e| format_err!("Failed while parsing {}: {:?}", import.path, e))?;

            // Search for the crate in which this .proto file exists.
            // TODO: Eventually this can be information that is communicated via the build
            // system

            let (mut rust_package_name, rust_package_dir) = {
                let mut rust_package = None;

                let mut current_dir = full_path.parent();
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
            for part in imported_proto_value.package.split(".") {
                package_path.push_str("::");
                package_path.push_str(escape_rust_identifier(part));
            }

            // TODO: Dedup this code.
            let file_name = c.resolve_file_name(&full_path)?;
            let file_id = {
                let id = crypto::sip::SipHasher::default_rounds_with_key_halves(0, 0)
                    .finish_with(file_name.as_bytes());
                base_radix::hex_encode(&id).to_ascii_uppercase()
            };

            c.imported_protos.push(ImportedProto {
                proto: imported_proto_value,
                package_path,
                file_id,
                file_path: import.path.clone(),
            });
        }

        c.outer.push_str("\n");

        // Add the file descriptor
        {
            let proto = c.compile_proto_descriptor(&file_name, &c.proto)?;

            let mut deps = vec![];
            for import in &c.imported_protos {
                deps.push(format!(
                    "// {}\n&{}::FILE_DESCRIPTOR_{},\n",
                    import.file_path, import.package_path, import.file_id
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

        let path: TypePath = &[];

        for def in &desc.definitions {
            let s = c.compile_topleveldef(def, &path)?;
            c.outer.push_str(&s);
            c.outer.push('\n');
        }

        Ok((c.outer, c.file_id))
    }

    fn resolve(&self, name_str: &str, path: TypePath) -> Option<ResolvedType> {
        let name = name_str.split('.').collect::<Vec<_>>();
        if name[0] == "" {
            panic!("Absolute paths currently not supported");
        }

        let mut package_path: Vec<&str> = self.proto.package.split('.').collect::<Vec<_>>();
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

    // TODO: Inline this.
    fn compile_enum_field(&self, f: &EnumField) -> String {
        format!("\t{} = {},", escape_rust_identifier(&f.name), f.num)
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

        let mut allow_alias = false;
        for i in &e.body {
            if let EnumBodyItem::Option(opt) = i {
                if opt.name == OptionName::Builtin("allow_alias".to_string()) {
                    match opt.value {
                        Constant::Bool(v) => allow_alias = v,
                        _ => {
                            panic!("Wrong type for allow_alias");
                        }
                    }
                }
            }
        }

        let mut lines = LineBuilder::new();

        // Because we can't put an enum inside of a struct in Rust, we instead
        // create a top level enum.
        // TODO: Need to consistently escape _'s in the original name.
        let fullname = {
            let mut inner_path = path.to_owned();
            inner_path.push(escape_rust_identifier(&e.name));
            inner_path.join("_")
        };

        let mut seen_numbers = HashMap::new();
        let mut duplicates = vec![];

        // TODO: Implement a better debug function
        lines.add("#[derive(Clone, Copy, PartialEq, Eq, Debug)]");
        lines.add(format!("pub enum {} {{", fullname));
        for i in &e.body {
            match i {
                EnumBodyItem::Option(_) => {}
                EnumBodyItem::Field(f) => {
                    if seen_numbers.contains_key(&f.num) {
                        if !allow_alias {
                            panic!("Duplicate enum value: {} in {}", f.num, e.name);
                        }

                        duplicates.push(f);
                        continue;
                    } else {
                        seen_numbers.insert(f.num, f);
                    }

                    lines.add(self.compile_enum_field(f));
                }
            }
        }
        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", fullname));
        for duplicate in duplicates {
            let main_field = seen_numbers.get(&duplicate.num).unwrap();

            lines.add(format!(
                "pub const {}: Self = Self::{};",
                escape_rust_identifier(&duplicate.name),
                escape_rust_identifier(&main_field.name)
            ));
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
                for i in &e.body {
                    match i {
                        EnumBodyItem::Option(_) => {}
                        EnumBodyItem::Field(f) => {
                            lines.add(format!("\t{} => {}::{},", f.num, fullname, f.name));
                        }
                    }
                }

                lines.add("\t_ => { return Err(WireError::UnknownEnumVariant); }");

                lines.add("})");
            });
            lines.add("}");
            lines.nl();

            // fn parse_name(&mut self, name: &str) -> Result<()>;
            lines.add("fn parse_name(s: &str) -> WireResult<Self> {");
            lines.add("\tOk(match s {");
            for i in &e.body {
                match i {
                    EnumBodyItem::Option(_) => {}
                    EnumBodyItem::Field(f) => {
                        lines.add(format!("\t\t\"{}\" => Self::{},", f.name, f.name));
                    }
                }
            }
            lines.add("_ => { return Err(WireError::UnknownEnumVariant); }");
            lines.add("})");
            lines.add("}");

            lines.add("fn name(&self) -> &'static str {");
            lines.add("\tmatch self {");
            for i in &e.body {
                match i {
                    EnumBodyItem::Option(_) => {}
                    EnumBodyItem::Field(f) => {
                        if seen_numbers[&f.num].name != f.name {
                            continue;
                        }

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

        lines.to_string()
    }

    fn compile_field_type(&self, typ: &FieldType, path: TypePath, options: &[Opt]) -> String {
        let max_length = {
            let mut size = None;
            for opt in options {
                if opt.name == OptionName::Builtin("max_length".to_string()) {
                    if let Constant::Integer(v) = opt.value {
                        size = Some(v as usize);
                    } else {
                        panic!("max_length option must be an integer");
                    }

                    break;
                }
            }

            size
        };

        if let Some(max_length) = max_length.clone() {
            if *typ == FieldType::Bytes {
                return format!("FixedVec<u8, {size}>", size = max_length);
            } else if *typ == FieldType::String {
                return format!("FixedString<[u8; {size}]>", size = max_length);
            } else {
                panic!("max_length not supported on type");
            }
        }

        String::from(match typ {
            FieldType::Double => "f64",
            FieldType::Float => "f32",
            FieldType::Int32 => "i32",
            FieldType::Int64 => "i64",
            FieldType::UInt32 => "u32",
            FieldType::UInt64 => "u64",
            FieldType::SInt32 => "i32",
            FieldType::SInt64 => "i64",
            FieldType::Fixed32 => "u32",
            FieldType::Fixed64 => "u64",
            FieldType::SFixed32 => "i32",
            FieldType::SFixed64 => "i64",
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
        escape_rust_identifier(&field.name)
    }

    fn field_name_inner(name: &str) -> &str {
        escape_rust_identifier(name)
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
                            if o.name == OptionName::Builtin("typed_num".to_string()) {
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
                                if o.name == OptionName::Builtin("typed_num".to_string()) {
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
            if opt.name == OptionName::Builtin("unordered_set".to_string()) {
                return true;
            }
        }

        false
    }

    fn compile_field(&mut self, field: &Field, path: TypePath) -> String {
        let mut s = String::new();
        s += self.field_name(field);
        s += ": ";

        let mut typ = self.compile_field_type(&field.typ, path, &field.unknown_options);

        let is_repeated = field.label == Label::Repeated;

        let max_count = {
            let mut size = None;
            for opt in &field.unknown_options {
                if opt.name == OptionName::Builtin("max_count".to_string()) {
                    if let Constant::Integer(v) = opt.value {
                        size = Some(v as usize);
                    } else {
                        panic!("max_count option must be an integer");
                    }

                    break;
                }
            }

            size
        };

        // We must box raw messages if they may cause cycles. It also simplifies support
        // for dynamic messages.
        // TODO: Do the same thing for groups.
        let is_message = self.is_message(&field.typ, path);
        if is_message && !self.is_unordered_set(field) && !self.is_primitive(&field.typ, path) {
            typ = format!("MessagePtr<{}>", typ);
        }

        /*
        Follow the nanopb convection:
        - max_length: For strings and bytes
        - max_count: For repeated fields.
        */

        if self.is_unordered_set(field) {
            s += &format!("{}::SetField<{}>", self.options.runtime_package, typ);
        } else if is_repeated {
            if let Some(max_count) = max_count {
                s += &format!("FixedVec<{typ}, {size}>", typ = typ, size = max_count);
            } else {
                s += &format!("Vec<{}>", &typ);
            }
        } else {
            if self.is_primitive(&field.typ, path) && self.proto.syntax == Syntax::Proto3 {
                s += &typ;
            } else {
                s += &format!("Option<{}>", typ);
            }
        }

        s += ",";
        s
    }

    fn oneof_typename(&self, oneof: &OneOf, path: TypePath) -> String {
        path.join("_") + escape_rust_identifier(&common::snake_to_camel_case(&oneof.name)) + "Case"
    }

    fn compile_oneof(&mut self, oneof: &OneOf, path: TypePath) -> CompiledOneOf {
        let mut lines = LineBuilder::new();

        let typename = self.oneof_typename(oneof, path);

        lines.add("#[derive(Clone, PartialEq)]");
        lines.add(r#"#[cfg_attr(feature = "alloc", derive(Debug))]"#);
        lines.add(format!("pub enum {} {{", typename));
        lines.add("\tNOT_SET,");
        for field in &oneof.fields {
            let mut typ = self.compile_field_type(&field.typ, path, &field.unknown_options);

            // TODO: Only do this for message types.
            if self.is_message(&field.typ, path) && !self.is_primitive(&field.typ, path) {
                typ = format!("MessagePtr<{}>", typ);
            }

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
                s += &self.compile_field_type(&f.key_type, path, &f.options);
                s += ", ";
                s += &self.compile_field_type(&f.value_type, path, &f.options);
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
                let enum_name = self.compile_field_type(typ, path, &[]);
                format!("{}::{}", enum_name, v)
            }
            Constant::Integer(v) => v.to_string(),
            Constant::Float(v) => v.to_string(),
            Constant::String(v) => {
                let mut out = String::new();
                serialize_str_lit(&v[..], &mut out);
                out
            }
            Constant::Bool(v) => if *v { "true" } else { "false" }.to_string(),
            Constant::Message(v) => {
                todo!()
            }
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
        let typ = self.compile_field_type(&field.typ, &path, &field.unknown_options);

        let is_primitive = self.is_primitive(&field.typ, &path);
        let is_copyable = self.is_copyable(&field.typ, &path);
        let is_message = self.is_message(&field.typ, &path);

        // NOTE: We

        let oneof_option = true; // !(is_primitive && self.proto.syntax == Syntax::Proto3);

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
            let max_count = {
                let mut size = None;
                for opt in &field.unknown_options {
                    if opt.name == OptionName::Builtin("max_count".to_string()) {
                        if let Constant::Integer(v) = opt.value {
                            size = Some(v as usize);
                        } else {
                            panic!("max_count option must be an integer");
                        }

                        break;
                    }
                }

                size
            };

            let mut typ = typ.clone();
            if is_message && !is_primitive {
                typ = format!("MessagePtr<{}>", typ);
            }

            let vec_type = {
                if let Some(max_count) = max_count {
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
                match &field.typ {
                    FieldType::String => "str",
                    FieldType::Bytes => "[u8]",
                    _ => &typ,
                }
            };

            // TODO: Need to read the 'default' property

            let explicit_default = field
                .unknown_options
                .iter()
                .find(|o| o.name == OptionName::Builtin("default".to_string()));

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
                        let oneof_typename = self.oneof_typename(oneof, path);
                        let oneof_fieldname = Self::field_name_inner(&oneof.name);
                        let oneof_case = common::snake_to_camel_case(&field.name);

                        let val = if is_message && !self.is_primitive(&field.typ, path) {
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
                    let oneof_typename = self.oneof_typename(oneof, path);
                    let oneof_fieldname = Self::field_name_inner(&oneof.name);
                    let oneof_case = common::snake_to_camel_case(&field.name);

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

        lines.to_string()
    }

    fn compile_map_field_accessors(&self, field: &MapField, path: TypePath) -> String {
        let key_type = self.compile_field_type(&field.key_type, path, &field.options);
        let value_type = self.compile_field_type(&field.value_type, path, &field.options);

        format!(
            r#"
            pub fn {name}(&self) -> &{pkg}::MapField<{key_type}, {value_type}> {{
                &self.{name}
            }}
            
            pub fn {name}_mut(&mut self) -> &{pkg}::MapField<{key_type}, {value_type}> {{
                &mut self.{name}
            }}
            "#,
            name = field.name, // TODO: Use a nice name.
            key_type = key_type,
            value_type = value_type,
            pkg = self.options.runtime_package
        )
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

        let mut can_have_extensions = false;

        let mut used_nums: HashSet<FieldNumber> = HashSet::new();
        for item in &msg.body {
            match item {
                MessageItem::Field(field) => {
                    if !used_nums.insert(field.num) {
                        panic!("Duplicate field number: {}", field.num);
                    }
                    if self.proto.syntax == Syntax::Proto3 {
                        // proto3 now allows optional fields again.

                        // if field.label != Label::None && field.label !=
                        // Label::Repeated {
                        //     panic!("Invalid field label in proto3");
                        // }
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
                    if option.name == OptionName::Builtin("typed_num".to_string()) {
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
                MessageItem::Extensions(e) => {
                    can_have_extensions = true;
                }
                _ => {}
            }
        }
        // TODO: Validate reserved field numbers/names.

        // TOOD: Debug with the enum code
        let mut fullname: String = {
            inner_path
                .iter()
                .map(|v| escape_rust_identifier(v))
                .collect::<Vec<_>>()
                .join("_")
        };

        let mut lines = LineBuilder::new();
        lines.add("#[derive(Clone, Default, PartialEq, ConstDefault)]");
        lines.add(format!("pub struct {} {{", fullname));
        lines.indented(|lines| -> Result<()> {
            for i in &msg.body {
                if let Some(field) = self.compile_message_item(&i, &inner_path)? {
                    lines.add(field);
                }
            }

            lines.add(format!(
                "unknown_fields: {}::UnknownFieldSet,",
                self.options.runtime_package
            ));

            if can_have_extensions {
                lines.add(format!(
                    "extensions: {}::ExtensionSet,",
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

            if let FieldType::Named(_) = field.typ {
                return Err(err_msg("typed_num message field must be a primitive type"));
            }

            let field_type = self.compile_field_type(&field.typ, &[], &field.unknown_options);

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
                field_name = escape_rust_identifier(&field.name)
            ));
        }

        lines.add(format!("impl {} {{", fullname));

        {
            let mut field_num_names = vec![];
            for item in &msg.body {
                match item {
                    MessageItem::OneOf(oneof) => {
                        for field in &oneof.fields {
                            field_num_names
                                .push((escape_rust_identifier(&field.name).to_string(), field.num));
                        }
                    }
                    MessageItem::Field(field) => {
                        field_num_names
                            .push((escape_rust_identifier(&field.name).to_string(), field.num));
                    }
                    MessageItem::MapField(field) => {
                        field_num_names
                            .push((escape_rust_identifier(&field.name).to_string(), field.num));
                    }
                    _ => {}
                }
            }

            for (field_name, field_num) in field_num_names {
                lines.add(format!(
                    "pub const {field_name}_FIELD_NUM: {pkg}::FieldNumber = {num};",
                    field_name = field_name.to_uppercase(),
                    pkg = self.options.runtime_package,
                    num = field_num
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

        for item in &msg.body {
            match item {
                MessageItem::OneOf(oneof) => {
                    let oneof_typename = self.oneof_typename(oneof, &inner_path);
                    lines.add(format!(
                        "
                        pub fn {name}_case(&self) -> &{ty} {{ &self.{field} }}
                        pub fn {name}_case_mut(&mut self) -> &mut {ty} {{ &mut self.{field} }}
                        ",
                        name = oneof.name,
                        ty = oneof_typename,
                        field = Self::field_name_inner(&oneof.name)
                    ));

                    for field in &oneof.fields {
                        lines.add(self.compile_field_accessors(field, &inner_path, Some(oneof)));
                    }

                    // Should also add fields to assign to each of the items
                }
                MessageItem::Field(field) => {
                    lines.add(self.compile_field_accessors(field, &inner_path, None));
                }
                MessageItem::MapField(field) => {
                    lines.add(self.compile_map_field_accessors(field, &inner_path));
                }
                _ => {}
            }
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

        let mut type_url_parts = vec![];
        if !self.proto.package.is_empty() {
            type_url_parts.push(self.proto.package.as_str());
        }
        type_url_parts.extend_from_slice(&inner_path);

        lines.add(format!(
            r#"impl {pkg}::Message for {name} {{
                
                fn type_url(&self) -> &str {{
                    "{type_url_prefix}{type_url}"
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
            type_url_prefix = protobuf_core::TYPE_URL_PREFIX,
            type_url = type_url_parts.join("."),
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
            let name = self.field_name(field);
            let is_repeated = field.label == Label::Repeated;

            let use_option = !(self.is_primitive(&field.typ, &inner_path)
                && self.proto.syntax == Syntax::Proto3);

            let is_message: bool = self.is_message(&field.typ, &inner_path);

            // TODO: Deduplicate this logic.
            let typeclass = match &field.typ {
                FieldType::Named(n) => {
                    // TODO: Call compile_field_type
                    let typ = self
                        .resolve(&n, &inner_path)
                        .expect(&format!("Failed to resolve type type: {}", n));

                    match &typ.descriptor {
                        ResolvedTypeDesc::Enum(_) => "Enum",
                        ResolvedTypeDesc::Message(_) => "Message",
                    }
                }
                _ => field.typ.as_str(),
            };

            // TODO: Must use repeated variants here.
            let mut p = String::new();
            if self.is_unordered_set(field) {
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
                                    ResolvedTypeDesc::Enum(_) => "Enum",
                                    ResolvedTypeDesc::Message(_) => "Message",
                                }
                            }
                            _ => field.typ.as_str(),
                        };

                        let mut value = format!("{}Codec::parse(&f)?", typeclass);
                        if typeclass == "Message" && !self.is_primitive(&field.typ, &inner_path) {
                            value = format!("MessagePtr::new(MessageCodec::parse(&f)?)");
                        }

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
        lines.add(
            r#"
            _ => {
                self.unknown_fields.fields.push(field_ref.span.into());
            }
        "#,
        );

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
            let name = self.field_name(field);
            let is_repeated = field.label == Label::Repeated;
            let is_message = self.is_message(&field.typ, &inner_path);

            // TODO: Dedup with above
            let typeclass = match &field.typ {
                FieldType::Named(n) => {
                    let typ = self
                        .resolve(&n, &inner_path)
                        .expect("Failed to resolve type");

                    match &typ.descriptor {
                        ResolvedTypeDesc::Enum(_) => "Enum",
                        ResolvedTypeDesc::Message(_) => "Message",
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
                serialize_method, field.num, reference_str, post_reference_str
            );

            if is_repeated {
                if self.is_unordered_set(field) {
                    // TODO: Support packed serialization of a SetField.
                    lines.add(format!(
                        "
                    for v in self.{name}.iter() {{
                        {typeclass}Codec::serialize({field_num}, {ref_str}v{post_str}, out)?;
                    }}
                    ",
                        typeclass = typeclass,
                        name = name,
                        field_num = field.num,
                        ref_str = reference_str,
                        post_str = post_reference_str,
                    ));
                } else {
                    lines.add(format!(
                        "{typeclass}Codec::serialize_repeated({field_num}, &self.{name}, out)?;",
                        typeclass = typeclass,
                        name = name,
                        field_num = field.num
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
                            field.num
                        ));
                    } else {
                        lines.add(serialize_line);
                    }
                    lines.add("\t\t}");
                } else {
                    // TODO: Should borrow the value when using messages
                    lines.add(format!(
                        "\t\t{}({}, {}self.{}{}, out)?;",
                        serialize_method, field.num, reference_str, name, post_reference_str
                    ));
                }

                if field.label == Label::Required {
                    lines.add_inline(" else {");
                    // TODO: Verify the field name doesn't have any quotes in it
                    lines.add(format!(
                        "\treturn Err(MessageSerializeError::RequiredFieldNotSet.into());"
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
                                    ResolvedTypeDesc::Enum(_) => "Enum",
                                    ResolvedTypeDesc::Message(_) => "Message",
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

                        let mut reference = if pass_reference { "" } else { "*" };

                        // Need to convert the &MessagePtr<Message> to an &Message
                        let post_reference = if typeclass == "Message"
                            && !self.is_primitive(&field.typ, &inner_path)
                        {
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
                            field_num = field.num
                        ));
                    }

                    lines.add(format!("\t\t\t{}::NOT_SET => {{}}", oneof_typename));

                    lines.add("\t\t}");
                }
                _ => {}
            }
        }

        lines.add(r#"self.unknown_fields.serialize_to(out)?;"#);
        if can_have_extensions {
            lines.add(r#"self.extensions.serialize_to(out)?;"#);
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
                
                fn unknown_fields(&self) -> &{pkg}::UnknownFieldSet {{
                    &self.unknown_fields
                }}


                fn extensions(&self) -> &{pkg}::ExtensionSet {{
                    {extensions}
                }}

                fn box_clone2(&self) -> Box<dyn ({pkg}::MessageReflection) + 'static> {{ 
                    Box::new(self.clone())
                }}

                 "#,
            pkg = self.options.runtime_package,
            name = fullname,
            extensions = {
                if can_have_extensions {
                    "&self.extensions".to_string()
                } else {
                    format!(
                        "
                        static DEFAULT: {pkg}::ExtensionSet = {pkg}::ExtensionSet::DEFAULT;
                        &DEFAULT
                    
                    ",
                        pkg = self.options.runtime_package
                    )
                }
            }
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

                            let f = match self.proto.syntax {
                                Syntax::Proto2 => "reflect_field_proto2",
                                Syntax::Proto3 => "reflect_field_proto3",
                            };

                            lines.add(format!("\t{} => self.{}.{}(),", field.num, name, f));
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
                                lines.add("\t\t\tSome(v.reflect())");

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
                            // TODO: Implement handling of None

                            let name = self.field_name(field);

                            let f = match self.proto.syntax {
                                Syntax::Proto2 => "reflect_field_mut_proto2",
                                Syntax::Proto3 => "reflect_field_mut_proto3",
                            };

                            lines.add(format!("\t{} => self.{}.{}(),", field.num, name, f));
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

                                let mut typ = self.compile_field_type(
                                    &field.typ,
                                    &inner_path,
                                    &field.unknown_options,
                                );

                                let is_message = self.is_message(&field.typ, &inner_path);
                                if is_message && !self.is_primitive(&field.typ, &inner_path) {
                                    typ = format!("MessagePtr<{}>", typ);
                                }

                                lines.add(format!(
                                    "\t\t\tself.{} = {}::{}(<{}>::default());",
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
            #[derive(Clone)]
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
                "fn file_descriptor(&self) -> &'static {}::StaticFileDescriptor {{ &FILE_DESCRIPTOR_{} }}",
                self.options.runtime_package, self.file_id
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

    fn resolve_file_name(&self, path: &LocalPath) -> Result<String> {
        let mut relative_path = None;
        for base_path in &self.options.paths {
            if let Some(p) = path.strip_prefix(base_path) {
                relative_path = Some(p);
                break;
            }
        }

        let relative_path = relative_path
            .ok_or_else(|| format_err!("Path is not in the protobuf paths: {:?}", path))?;

        Ok(relative_path.to_string())
    }

    /// Generates Rust code which produces a '[u8]' value containing a
    /// FileDescriptorProto for 'proto'.
    ///
    /// Arguments:
    /// - path: The absolute path from which we read 'proto'.
    /// - proto: A parsed .proto file.
    #[cfg(feature = "descriptors")]
    fn compile_proto_descriptor(&self, file_name: &str, proto: &Proto) -> Result<String> {
        let mut p = proto.to_proto();
        p.set_name(file_name);

        let data = p.serialize()?;
        // assert_eq!(p, protobuf_descriptor::FileDescriptorProto::parse(&data)?);

        Ok(rust_bytestring(&data))
    }

    #[cfg(not(feature = "descriptors"))]
    fn compile_proto_descriptor(&self, file_name: &str, proto: &Proto) -> Result<String> {
        Ok(rust_bytestring(&[]))
    }
}
