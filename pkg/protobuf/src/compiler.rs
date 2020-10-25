// Code for taking a parsed .proto file descriptor and performing code
// generation into Rust code.

use super::spec::*;
use common::line_builder::*;

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

// Roughly similar to the descriptor database in the regular protobuf library
// Stores all parsed .proto files currently loaded
struct DescriptorDatabase {
    base_dir: String,
}

enum ResolvedTypeDesc<'a> {
    Message(&'a Message),
    Enum(&'a Enum),
}

struct ResolvedType<'a> {
    // Name of the type in the currently being compiled source file.
    typename: String,
    descriptor: ResolvedTypeDesc<'a>,
}

pub struct Compiler<'a> {
    // The current top level code string that we are building.
    outer: String,

    // Top level proto file descriptor that is being compiled
    proto: &'a Proto,
    // TODO: Will also need a DescriptorDatabase to look up items in other files
}

/*
    TODO:
    Things to validate about a proto file
    - All definitions at the same level have distinct names
    - Enum fields and message fields have have distinct names
    - All message fields have distinct numbers
*/

type Path<'a> = &'a [&'a str];

trait Resolvable {
    fn resolve(&self, path: Path) -> Option<ResolvedType>;
}

impl Resolvable for Message {
    fn resolve(&self, path: Path) -> Option<ResolvedType> {
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
        for def in &self.definitions {
            let inner = match def {
                TopLevelDef::Enum(e) => e.resolve(&path),
                TopLevelDef::Message(m) => m.resolve(&path),
                _ => None,
            };

            if inner.is_some() {
                return inner;
            }
        }

        None
    }
}

impl Compiler<'_> {
    pub fn compile(desc: &Proto) -> String {
        let mut c = Compiler {
            outer: String::new(),
            proto: desc,
        };

        c.outer += "// AUTOGENERATED BY PROTOBUF COMPILER\n\n";
        c.outer += "use std::sync::Arc;\n";
        c.outer += "use common::errors::*;\n";
        c.outer += "use protobuf::*;\n";
        c.outer += "use protobuf::wire::*;\n";
        c.outer += "use protobuf::service::*;\n";
        c.outer += "use protobuf::reflection::*;\n\n";

        // TODO: Eventually, this will become the package name
        let path = vec![];

        for def in &desc.definitions {
            let s = c.compile_topleveldef(def, &path);
            c.outer.push_str(&s);
            c.outer.push('\n');
        }

        c.outer
    }

    fn resolve(&self, name_str: &str, mut path: Path) -> Option<ResolvedType> {
        let name = name_str.split('.').collect::<Vec<_>>();
        if name[0] == "" {
            panic!("Absolute paths currently not supported");
        }

        loop {
            let mut fullname = Vec::from(path);
            fullname.extend_from_slice(&name);

            let t = self.proto.resolve(&fullname);
            if t.is_some() {
                return t;
            }

            if path.len() == 0 {
                break;
            }

            // For path 'x.y.z', try 'x.y' next time.
            path = &path[0..(path.len() - 1)];
        }

        None
    }

    fn compile_enum_field(&self, f: &EnumField) -> String {
        format!("\t{} = {},", f.name, f.num)
    }

    fn compile_enum(&self, e: &Enum, path: Path) -> String {
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
        let fullname = format!("{}_{}", path.join("_"), e.name);

        lines.add("#[derive(Clone, Copy, PartialEq, Eq)]");
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

        lines.add(format!("impl Enum for {} {{", fullname));
        lines.indented(|lines| {
            // TODO: Just make from_usize an Option<>
            lines.add("fn from_usize(v: usize) -> std::result::Result<Self, ()> {");

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

                lines.add("\t_ => { return Err(()); }");

                lines.add("})");
            });

            lines.add("}");
            lines.nl();

            lines.add("fn to_usize(&self) -> usize { self as usize }");
            lines.nl();

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

            lines.add("fn parse(&mut self, v: usize) -> Result<()> {");
            lines.add("\t*self = Self::from_usize(v)?; Ok(())");
            lines.add("}");
            lines.nl();

            lines.add("fn parse_str(&mut self, s: &str) -> Result<()> {");
            lines.add("\t*self = Self::from_str(s)?; Ok(())");
            lines.add("}");
            lines.nl();

            // fn parse_name(&mut self, name: &str) -> Result<()>;
            lines.add("fn from_str(s: &str) -> Result<Self> {");
            lines.add("\tOk(match s {");
            for i in &e.body {
                match i {
                    EnumBodyItem::Option(_) => {}
                    EnumBodyItem::Field(f) => {
                        lines.add(format!("\t\t\"{}\" => Self::{},", f.name, f.name));
                    }
                }
            }
            lines.add("_ => { return Err(()); }");
            lines.add("})");
            lines.add("}");
        });
        lines.add("}");

        lines.to_string()
    }

    fn compile_field_type(&self, typ: &FieldType, path: Path) -> String {
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
            FieldType::Bytes => "BytesMut",
            // TODO: This must resolve the right module (and do any nesting
            // conversions needed)
            // ^ There
            FieldType::Named(s) => {
                return self
                    .resolve(&s, path)
                    .expect("Failed to resolve type")
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

    /// Checks if a type is a 'primitive'.
    ///
    /// A primitive is defined mainly as anything but a nested message type.
    /// In proto3, the presence of primitive fields is undefined.
    fn is_primitive(&self, typ: &FieldType, path: Path) -> bool {
        if let FieldType::Named(name) = typ {
            let resolved = self.resolve(name, path).expect("");
            match resolved.descriptor {
                ResolvedTypeDesc::Enum(_) => true,
                ResolvedTypeDesc::Message(_) => false,
            }
        } else {
            true
        }
    }

    fn compile_field(&self, field: &Field, path: Path) -> String {
        let mut s = String::new();
        s += "\t";
        s += self.field_name(field);
        s += ": ";

        let typ = self.compile_field_type(&field.typ, path);

        let is_repeated = if field.label == Label::Repeated {
            true
        } else {
            false
        };
        if is_repeated {
            s += &format!("Vec<{}>", typ);
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

    /// Compiles a single
    fn compile_message_item(&mut self, item: &MessageItem, path: Path) -> Option<String> {
        match item {
            MessageItem::Enum(e) => {
                self.outer.push_str(&self.compile_enum(e, path));
                None
            }
            // MessageItem::Message(msg) => Self::compile_message(outer, msg),
            MessageItem::Field(f) => Some(self.compile_field(f, path)),
            _ => None,
        }
    }

    fn compile_message(&mut self, msg: &Message, path: Path) -> String {
        // TODO: Create a ConstDefault version of the message (should be used if someone
        // wants to access an uninitialized message?)

        // TODO: Complain if we include a proto2 enum directly as a field of a proto3
        // message.

        let mut inner_path = Vec::from(path);
        inner_path.push(&msg.name);

        let mut lines = LineBuilder::new();
        // TODO: Use the debug_string to implement debug.
        lines.add("#[derive(Clone, Default, Debug)]");
        lines.add(format!("pub struct {} {{", msg.name));
        for i in &msg.body {
            if let Some(field) = self.compile_message_item(&i, &inner_path) {
                lines.add(field);
            }
        }

        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", msg.name));
        for field in msg.fields() {
            let name = self.field_name(field);

            // TODO: Verify the given label is allowed in the current syntax
            // version

            let is_repeated = field.label == Label::Repeated;
            let typ = self.compile_field_type(&field.typ, &inner_path);

            let is_primitive = self.is_primitive(&field.typ, path);

            // TODO: Messages should always have options?
            let use_option = !(field.label == Label::Required
                || (self.is_primitive(&field.typ, path) && self.proto.syntax == Syntax::Proto3));

            // field()
            if is_repeated {
                lines.add(format!("\tpub fn {}(&self) -> &[{}] {{", name, typ));
                lines.add_inline(format!(" &self.{}", name));
                lines.add_inline(" }");
            } else {
                // TODO: For Option<>, we need a &'static thing to use.
                // For primitives, it is sufficient to copy it.
                lines.add(format!("\tpub fn {}(&self) -> &{} {{", name, typ));
                lines.add_inline(format!(" &self.{} }}", name));
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

            if is_repeated {
                // add_field(v: T) -> &mut T
                lines.add(format!(
                    "\tpub fn add_{}(&mut self, v: {}) -> &mut {} {{",
                    name, typ, typ
                ));
                lines.add(format!(
                    "\t\tself.{}.push(v); self.{}.last().unwrap()",
                    name, name
                ));
                lines.add("\t}");

            // NOTE: We do not define 'fn add_field() -> &mut T'
            } else {
                if
                /* is_primitive */
                true {
                    // set_field(v: T)
                    lines.add(format!("\tpub fn set_{}(&mut self, v: {}) {{", name, typ));
                    lines.add(format!("\t\tself.{} = v;", name));
                    lines.add("\t}");
                }

                // TODO: For Option<>, must set it to be a Some(Type::default())
                // ^ Will also need to
                // field_mut() -> &mut T
                lines.add(format!(
                    "\tpub fn {}_mut(&mut self) -> &mut {} {{",
                    name, typ
                ));
                lines.add_inline(format!(" &mut self.{} }}", name));
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
        }

        lines.add("}");
        lines.nl();

        lines.add(format!("impl protobuf::Message for {} {{", msg.name));
        lines.add("\tfn parse(data: Bytes) -> Result<Self> {");
        lines.add("\t\tlet mut msg = Self::default();");
        lines.add("\t\tlet fields = WireField::parse_all(&data)?;");
        lines.add("\t\tfor f in &fields {");
        lines.add("\t\t\tmatch f.field_number {");

        for field in msg.fields() {
            let name = self.field_name(field);
            let is_repeated = field.label == Label::Repeated;

            let use_option =
                !(self.is_primitive(&field.typ, path) && self.proto.syntax == Syntax::Proto3);

            let typename = match &field.typ {
                FieldType::Named(n) => {
                    let typ = self
                        .resolve(&n, &inner_path)
                        .expect("Failed to resolve type");

                    match &typ.descriptor {
                        ResolvedTypeDesc::Enum(e) => "enum",
                        ResolvedTypeDesc::Message(m) => "message",
                    }
                }
                _ => field.typ.as_str(),
            };

            let mut p = String::new();
            if is_repeated {
                p += &format!("msg.{}.push(f.parse_{}()?)", name, typename);
            } else {
                if use_option {
                    p += &format!("msg.{} = Some(f.parse_{}()?)", name, typename);
                } else {
                    p += &format!("msg.{} = f.parse_{}()?", name, typename);
                }
            }

            lines.add(format!("\t\t\t\t{} => {{ {} }},", field.num, p));
        }

        // TODO: Will need to record this as an unknown field.
        lines.add("\t\t\t\t_ => {}");

        lines.add("\t\t\t}");
        lines.add("\t\t}");
        lines.add("\t\tOk(msg)");
        lines.add("\t}");

        lines.add("\tfn serialize(&self) -> Result<Vec<u8>> {");
        lines.add("\t\tlet mut data = vec![];");

        for field in msg.fields() {
            let name = self.field_name(field);
            let is_repeated = field.label == Label::Repeated;

            // TODO: Dedup with above
            let typename = match &field.typ {
                FieldType::Named(n) => {
                    let typ = self
                        .resolve(&n, &inner_path)
                        .expect("Failed to resolve type");

                    match &typ.descriptor {
                        ResolvedTypeDesc::Enum(e) => "enum",
                        ResolvedTypeDesc::Message(m) => "message",
                    }
                }
                _ => field.typ.as_str(),
            };

            let use_option =
                !(self.is_primitive(&field.typ, path) && self.proto.syntax == Syntax::Proto3);

            let serialize_line = format!(
                "\t\t\tWireField::serialize_{}({}, v, &mut data)?;",
                typename, field.num
            );

            if is_repeated {
                lines.add(format!("\t\tfor v in &self.{} {{", name));
                lines.add(serialize_line);
                lines.add("\t\t}");
            } else {
                // TODO: For proto3, the requirement is that it is not equal to
                // the default value (and there would be no optional for
                if use_option {
                    lines.add(format!("\t\tif let Some(v) = self.{} {{", name));
                    lines.add(serialize_line);
                    lines.add("\t\t}");
                } else {
                    // TODO: Should borrow the value when using messages
                    lines.add(format!(
                        "\t\tWireField::serialize_{}({}, self.{}, &mut data);",
                        typename, field.num, name
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

        lines.add("\t\tOk(data)");
        lines.add("\t}");

        // Implementing merge_from
        // extend_from_slice and assignment

        lines.add("}");
        lines.nl();

        lines.add(format!(
            "impl protobuf::MessageReflection for {} {{",
            msg.name
        ));

        lines.indented(|lines| {
            lines.add("fn field_by_number(&self, num: FieldNumber) -> Option<Reflection> {");
            lines.indented(|lines| {
                lines.add("Some(match num {");
                for field in msg.fields() {
                    let name = self.field_name(field);
                    lines.add(format!("\t{} => self.{}.reflect(),", field.num, name));
                }
                lines.add("\t_ => { return None; }");
                lines.add("})");
            });
            lines.add("}");
            lines.nl();

            // TODO: Dedup with the last case.
            lines.add(
                "fn field_by_number_mut(&mut self, num: FieldNumber) -> Option<ReflectionMut> {",
            );
            lines.indented(|lines| {
                lines.add("Some(match num {");
                for field in msg.fields() {
                    let name = self.field_name(field);
                    lines.add(format!("\t{} => self.{}.reflect_mut(),", field.num, name));
                }
                lines.add("\t_ => { return None; }");
                lines.add("})");
            });
            lines.add("}");
            lines.nl();

            lines.add("fn field_number_by_name(&self, name: &str) -> Option<FieldNumber> {");
            lines.indented(|lines| {
                lines.add("Some(match name {");
                for field in msg.fields() {
                    let name = self.field_name(field);
                    lines.add(format!("\t{} => {},", name, field.num));
                }
                lines.add("})");
            });
            lines.add("}");
        });

        lines.add("}");

        lines.to_string()
    }

    fn compile_service(&mut self, service: &Service, path: Path) -> String {
        //		let modname = common::camel_to_snake_case(&service.name);

        let mut lines = LineBuilder::new();

        // Full name of the service including the package name
        // e.g. google.api.MyService
        let absolute_name: String = {
            let mut parts = path.to_vec();
            parts.push(service.name.as_str());
            parts.join(".")
        };

        lines.add(format!("pub struct {}Stub {{", service.name));
        lines.add("\tchannel: Arc<dyn Channel>");
        lines.add("}");
        lines.nl();

        lines.add(format!("impl {}Stub {{", service.name));
        lines.indented(|lines| {
            lines.add("pub fn new(channel: Arc<dyn Channel>) -> Self {");
            lines.add("\tSelf { channel }");
            lines.add("}");
            lines.nl();

            for rpc in service.rpcs() {
                lines.add(format!(
                    "pub async fn {}(&self, request: &{}) -> Result<{}> {{",
                    rpc.name, rpc.req_type, rpc.res_type
                ));
                lines.add(format!(
                    "\tlet response_bytes = self.channel.call(\
						\"{}\", \"{}\", request.serialize()?.into()).await?;",
                    absolute_name, rpc.name
                ));
                lines.add(format!("\t{}::parse(response_bytes)", rpc.res_type));
                lines.add("}");
                lines.nl();
            }
        });
        lines.add("}");
        lines.nl();

        lines.add("#[async_trait]");
        lines.add(format!("pub trait {}Service: Send + Sync {{", service.name));

        for rpc in service.rpcs() {
            let req_type = self
                .resolve(&rpc.req_type, path)
                .expect(&format!("Failed to find {}", rpc.req_type));
            let res_type = self.resolve(&rpc.res_type, path).expect("");

            // TODO: Must resolve the typename.
            lines.add(format!(
                "\tasync fn {}(&self, request: {}",
                rpc.name,
                if rpc.req_stream {
                    format!("&dyn InputStream<{}>", req_type.typename)
                } else {
                    req_type.typename
                }
            ));

            if rpc.res_stream {
                lines.add_inline(format!(
                    ", response: &dyn Sink<{}>) -> Result<()>;",
                    res_type.typename
                ));
            } else {
                lines.add_inline(format!(") -> Result<{}>;", res_type.typename));
            }
        }

        lines.nl();

        lines.add("\tfn into_service(self) -> Arc<dyn Service> where Self: 'static + Sized {");
        lines.add(format!("\t\tArc::new({}ServiceCaller {{", service.name));
        lines.add(format!(
            "\t\t\tinner: Box::new(self) as Box<dyn {}Service>",
            service.name
        ));
        lines.add("\t\t})");
        lines.add("\t}");

        lines.add("}");
        lines.nl();

        lines.add(format!(
            "pub struct {}ServiceCaller {{ inner: Box<dyn {}Service> }}",
            service.name, service.name
        ));
        lines.nl();

        lines.add("#[async_trait]");
        lines.add(format!("impl Service for {}ServiceCaller {{", service.name));
        lines.indented(|lines| {
            // TODO: Escape the string if needed.
            lines.add(format!(
                "fn service_name(&self) -> &'static str {{ \"{}\" }}",
                absolute_name
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

            lines.add(
                "async fn call(&self, method_name: &str, \
					   request_bytes: Bytes) -> Result<Bytes> {",
            );

            lines.indented(|lines| {
                lines.add("match method_name {");

                for rpc in service.rpcs() {
                    lines.add(format!("\t\"{}\" => {{", rpc.name));
                    // TODO: Resolve type names.
                    lines.add(format!(
                        "\t\tlet request = {}::parse(request_bytes)?;",
                        rpc.req_type
                    ));
                    // TODO: Must normalize these names to valid Rust names
                    lines.add(format!(
                        "\t\tlet response = self.inner.{}(request).await?;",
                        rpc.name
                    ));
                    lines.add("\t\tOk(response.serialize()?.into())");
                    lines.add("\t},");
                }

                lines.add("\t_ => Err(err_msg(\"Invalid method\"))");
                lines.add("}");
            });

            lines.add("}");
        });
        lines.add("}");
        lines.nl();

        // impl Service for MyService {

        lines.to_string()
    }

    fn compile_topleveldef(&mut self, def: &TopLevelDef, path: Path) -> String {
        match def {
            TopLevelDef::Message(m) => self.compile_message(&m, path),
            TopLevelDef::Enum(e) => self.compile_enum(e, path),
            TopLevelDef::Service(s) => self.compile_service(s, path),
            _ => String::new(),
        }
    }
}
