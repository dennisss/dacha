// This file contains structs which define the syntax tree of a .proto file
// describing a set of messages/services.

use alloc::vec::Vec;
use std::string::{String, ToString};

use protobuf_core::text::TextMessage;
use protobuf_core::{EnumValue, FieldNumber};
use protobuf_descriptor as pb;

// Proto 2 and 3
#[derive(Clone, Debug)]
pub enum Constant {
    Identifier(String),
    Integer(i64),
    Float(f64),
    String(Vec<u8>),
    Bool(bool),
    Message(TextMessage),
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum Syntax {
    Proto2,
    Proto3,
}

#[derive(PartialEq, Debug, Clone)]
pub enum ImportType {
    Default,
    Weak,
    Public,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub typ: ImportType,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct Opt {
    pub name: OptionName,
    pub value: Constant,
}

impl Opt {
    fn to_proto(&self) -> pb::UninterpretedOption {
        let mut proto = pb::UninterpretedOption::default();

        match &self.name {
            OptionName::Builtin(name) => {
                let n = proto.new_name();
                n.set_name_part(name);
                n.set_is_extension(false);
            }
            OptionName::Custom {
                extension_name,
                field,
            } => {
                let n1 = proto.new_name();
                n1.set_name_part(extension_name);
                n1.set_is_extension(true);

                if let Some(field) = field {
                    let n2 = proto.new_name();
                    n2.set_name_part(field);
                    n2.set_is_extension(false);
                }
            }
        }

        match &self.value {
            Constant::Identifier(v) => proto.set_identifier_value(v),
            Constant::Integer(v) => {
                if *v < 0 {
                    proto.set_negative_int_value(*v);
                } else {
                    proto.set_positive_int_value(*v as u64);
                }
            }
            Constant::Float(v) => {
                proto.set_double_value(*v);
            }
            Constant::String(v) => {
                proto.set_string_value(&v[..]);
            }
            Constant::Bool(v) => {
                proto.set_identifier_value(v.to_string());
            }
            Constant::Message(v) => {
                proto.set_aggregate_value(v.to_string());
            }
        }

        proto
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OptionName {
    Builtin(String),
    Custom {
        // TODO: Make sure we support this starting with a '.' to be absolute
        extension_name: String,
        field: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Label {
    None,
    Required,
    Optional,
    Repeated,
}

impl Label {
    fn to_proto(&self) -> pb::FieldDescriptorProto_Label {
        match self {
            Label::None => pb::FieldDescriptorProto_Label::LABEL_OPTIONAL,
            Label::Required => pb::FieldDescriptorProto_Label::LABEL_REQUIRED,
            Label::Optional => pb::FieldDescriptorProto_Label::LABEL_OPTIONAL,
            Label::Repeated => pb::FieldDescriptorProto_Label::LABEL_REPEATED,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Double,
    Float,
    Int32,
    Int64,
    UInt32,
    UInt64,
    SInt32,
    SInt64,
    Fixed32,
    Fixed64,
    SFixed32,
    SFixed64,
    Bool,
    String,
    Bytes,
    /// Either a message or enum type.
    /// // TODO: Make sure we support this starting with a '.' to be absolute
    Named(String),
}

impl FieldType {
    /*
    /// Whether or not this type is a primitive (non-enum
    /// TODO: Need to check if it would be an enum
    pub fn is_primitive(&self) -> bool {
        if let FieldType::Named(_) = self { false } else { true }
    }
    */
}

#[derive(Debug, Clone)]
pub struct Field {
    pub label: Label,
    pub typ: FieldType,
    pub name: String,
    pub num: FieldNumber,
    pub options: Vec<Opt>,
}

impl Field {
    fn to_proto(&self, oneof_index: Option<usize>, syntax: Syntax) -> pb::FieldDescriptorProto {
        let mut proto = pb::FieldDescriptorProto::default();
        proto.set_name(&self.name);
        proto.set_number(self.num as i32);
        proto.set_label(self.label.to_proto());

        if self.label == Label::Optional && syntax == Syntax::Proto3 {
            proto.set_proto3_optional(true);
        }

        match &self.typ {
            FieldType::Double => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_DOUBLE),
            FieldType::Float => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_FLOAT),
            FieldType::Int32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_INT32),
            FieldType::Int64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_INT64),
            FieldType::UInt32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_UINT32),
            FieldType::UInt64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_UINT64),
            FieldType::SInt32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_SINT32),
            FieldType::SInt64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_SINT64),
            FieldType::Fixed32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_FIXED32),
            FieldType::Fixed64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_FIXED64),
            FieldType::SFixed32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_SFIXED32),
            FieldType::SFixed64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_SFIXED64),
            FieldType::Bool => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_BOOL),
            FieldType::String => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_STRING),
            FieldType::Bytes => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_BYTES),
            FieldType::Named(name) => {
                // TODO: How do we distinguish between an enum and a message?
                proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_MESSAGE);
                proto.set_type_name(name);
            }
        }

        if let Some(idx) = oneof_index {
            proto.set_oneof_index(idx as i32);
        }

        for opt in &self.options {
            proto.options_mut().add_uninterpreted_option(opt.to_proto());
        }

        proto
    }
}

// Proto 2
#[derive(Debug, Clone)]
pub struct Group {
    pub label: Label,
    pub name: String,
    pub num: FieldNumber,
    pub body: Vec<MessageItem>,
}

#[derive(Debug, Clone)]
pub struct OneOf {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct MapField {
    pub key_type: FieldType,
    pub value_type: FieldType,
    pub name: String,
    pub num: FieldNumber,
    pub options: Vec<Opt>,
}

pub type Ranges = Vec<Range>;

// Upper and lower bounds are inclusive.
pub type Range = (FieldNumber, FieldNumber);

#[derive(Debug, Clone)]
pub enum Reserved {
    Ranges(Ranges),
    // Names of fields that are not allowed to appear again in the message.
    Fields(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub name: String,
    pub body: Vec<EnumBodyItem>,
}

impl Enum {
    fn to_proto(&self) -> pb::EnumDescriptorProto {
        let mut proto = pb::EnumDescriptorProto::default();
        proto.set_name(&self.name);
        for item in &self.body {
            match item {
                EnumBodyItem::Option(opt) => {
                    proto.options_mut().add_uninterpreted_option(opt.to_proto());
                }
                EnumBodyItem::Field(field) => {
                    let mut v = pb::EnumValueDescriptorProto::default();
                    v.set_name(&field.name);
                    v.set_number(field.num);
                    proto.add_value(v);
                }
            }
        }

        proto
    }
}

#[derive(Debug, Clone)]
pub enum EnumBodyItem {
    Option(Opt),
    Field(EnumField),
}

#[derive(Debug, Clone)]
pub struct EnumField {
    pub name: String,
    pub num: EnumValue,
    pub options: Vec<Opt>,
}

#[derive(Debug, Clone)]
pub struct MessageDescriptor {
    pub name: String,
    pub body: Vec<MessageItem>,
}

impl MessageDescriptor {
    fn to_proto(&self, syntax: Syntax) -> pb::DescriptorProto {
        let mut proto = pb::DescriptorProto::default();
        proto.set_name(&self.name);

        for item in &self.body {
            match item {
                MessageItem::Field(f) => {
                    proto.add_field(f.to_proto(None, syntax));
                }
                MessageItem::Enum(e) => {
                    proto.add_enum_type(e.to_proto());
                }
                MessageItem::OneOf(o) => {
                    let idx = proto.oneof_decl_len();

                    let mut v = pb::OneofDescriptorProto::default();
                    v.set_name(&o.name);
                    proto.add_oneof_decl(v);

                    for field in &o.fields {
                        proto.add_field(field.to_proto(Some(idx), syntax));
                    }
                }
                MessageItem::Message(m) => {
                    proto.add_nested_type(m.to_proto(syntax));
                }
                MessageItem::MapField(f) => {
                    let mut entry = pb::DescriptorProto::default();
                    entry.set_name(format!("{}Entry", common::snake_to_camel_case(&f.name)));
                    entry.options_mut().set_map_entry(true);
                    entry.add_field(
                        Field {
                            label: Label::Optional,
                            typ: f.key_type.clone(),
                            name: "key".to_string(),
                            num: 1, // TODO: Define this in some constants file
                            options: vec![],
                        }
                        .to_proto(None, syntax),
                    );
                    entry.add_field(
                        Field {
                            label: Label::Optional,
                            typ: f.value_type.clone(),
                            name: "value".to_string(),
                            num: 2, // TODO: Define this in some constants file
                            options: vec![],
                        }
                        .to_proto(None, syntax),
                    );

                    proto.add_field(
                        Field {
                            label: Label::Repeated,
                            typ: FieldType::Named(entry.name().to_string()),
                            name: f.name.to_string(),
                            num: f.num,
                            options: vec![],
                        }
                        .to_proto(None, syntax),
                    );

                    proto.add_nested_type(entry);
                }
                MessageItem::Extensions(e) => {
                    for (start, end) in e.iter() {
                        let mut r = pb::DescriptorProto_ExtensionRange::default();
                        r.set_start(*start as i32);
                        // if *end != std::usize::MAX {
                        r.set_end(*end as i32);
                        // }
                        proto.add_extension_range(r);
                    }
                }
                MessageItem::Reserved(r) => {
                    // TODO
                }
                MessageItem::Option(opt) => {
                    proto.options_mut().add_uninterpreted_option(opt.to_proto());
                }
                MessageItem::Extend(v) => {
                    for e in v.to_proto(syntax) {
                        proto.add_extension(e);
                    }
                }
                v @ _ => {
                    println!("Do not support {:?}", v);
                    todo!()
                }
            }
        }

        proto
    }

    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.body.iter().filter_map(|item| {
            if let MessageItem::Field(f) = item {
                Some(f)
            } else {
                None
            }
        })
    }
}

#[derive(Debug, Clone)]
pub enum MessageItem {
    Field(Field),
    Enum(Enum),
    Message(MessageDescriptor),
    Extend(Extend),
    Extensions(Ranges),
    Group(Group),
    Option(Opt),
    OneOf(OneOf),
    MapField(MapField),
    Reserved(Reserved),
}

#[derive(Debug, Clone)]
pub enum ExtendItem {
    Field(Field),
    // Group(Group),
}

#[derive(Debug, Clone)]
pub struct Extend {
    pub typ: String,
    pub body: Vec<ExtendItem>,
}

impl Extend {
    fn to_proto(&self, syntax: Syntax) -> Vec<pb::FieldDescriptorProto> {
        let mut out = vec![];

        for item in &self.body {
            match item {
                ExtendItem::Field(field) => {
                    let mut proto = field.to_proto(None, syntax);
                    proto.set_extendee(&self.typ);
                    out.push(proto);
                }
            }
        }

        out
    }
}

#[derive(Debug, Clone)]
pub enum ServiceItem {
    Option(Opt),
    RPC(RPC),
    Stream(Stream),
}

#[derive(Debug, Clone)]
pub struct Service {
    pub name: String,
    pub body: Vec<ServiceItem>,
}

impl Service {
    fn to_proto(&self) -> pb::ServiceDescriptorProto {
        let mut proto = pb::ServiceDescriptorProto::default();
        proto.set_name(&self.name);
        for item in &self.body {
            match item {
                ServiceItem::RPC(rpc) => {
                    proto.add_method(rpc.to_proto());
                }
                ServiceItem::Option(_) => {
                    //
                }
                _ => todo!(),
            }
        }

        proto
    }
}

// TODO: This should be straight forward to just replace with a
// pb::MethodDescriptorProto usage.
#[derive(Debug, Clone)]
pub struct RPC {
    pub name: String,
    pub req_type: String,
    pub req_stream: bool,
    pub res_type: String,
    pub res_stream: bool,
    pub options: Vec<Opt>,
}

impl RPC {
    fn to_proto(&self) -> pb::MethodDescriptorProto {
        let mut proto = pb::MethodDescriptorProto::default();
        proto.set_name(&self.name);
        proto.set_input_type(&self.req_type);
        proto.set_output_type(&self.res_type);
        proto.set_client_streaming(self.req_stream);
        proto.set_server_streaming(self.res_stream);

        for opt in &self.options {
            proto.options_mut().add_uninterpreted_option(opt.to_proto());
        }

        proto
    }
}

#[derive(Debug, Clone)]
pub struct Stream {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub options: Vec<Opt>,
}

/*
- Basically convert one of these into a FileDescriptorProto.
- When compiling a file, we will put a "const FILE_DESCRIPTOR: &'static [u8]" at the top which contains the serialized proto
    - This also needs to be aware of all files it references (so may recursively reference the descriptors in other packages).
- A protobuf_core::Message will have a file_descriptor() method to get the serialized descriptor of the file.
- An rpc::Service will need to be able to register all types it uses into a descriptor pool upon request.
*/

// Basically must be able to convert this to a FileDescriptorProto
#[derive(Debug, Clone)]
pub struct Proto {
    pub syntax: Syntax,
    pub package: String,
    pub imports: Vec<Import>,
    pub definitions: Vec<TopLevelDef>,
}

impl Proto {
    pub fn to_proto(&self) -> pb::FileDescriptorProto {
        let mut proto = pb::FileDescriptorProto::default();
        proto.set_syntax(match self.syntax {
            Syntax::Proto2 => "proto2",
            Syntax::Proto3 => "proto3",
        });

        proto.set_package(&self.package);

        // TODO: Ensure that these are relative to the root of the file
        for import in &self.imports {
            let idx = proto.dependency_len() as i32;
            proto.add_dependency(import.path.clone());
            match import.typ {
                ImportType::Default => {}
                ImportType::Weak => {
                    proto.add_weak_dependency(idx);
                }
                ImportType::Public => {
                    proto.add_public_dependency(idx);
                }
            }
        }

        for def in &self.definitions {
            match def {
                TopLevelDef::Message(m) => {
                    proto.add_message_type(m.to_proto(self.syntax));
                }
                TopLevelDef::Enum(e) => {
                    proto.add_enum_type(e.to_proto());
                }
                TopLevelDef::Extend(e) => {
                    for e in e.to_proto(self.syntax) {
                        proto.add_extension(e);
                    }
                }
                TopLevelDef::Service(s) => {
                    proto.add_service(s.to_proto());
                }
                TopLevelDef::Option(opt) => {
                    proto.options_mut().add_uninterpreted_option(opt.to_proto());
                }
            }
        }

        proto
    }
}

#[derive(Debug, Clone)]
pub enum TopLevelDef {
    Message(MessageDescriptor),
    Enum(Enum),
    Extend(Extend),
    Service(Service),
    Option(Opt),
}

pub struct Positioned {}
