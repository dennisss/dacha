// This file contains structs which define the syntax tree of a .proto file
// describing a set of messages/services.

use protobuf_core::{EnumValue, FieldNumber};
use protobuf_descriptor as pb;

// Proto 2 and 3
#[derive(Clone, Debug)]
pub enum Constant {
    Identifier(String),
    Integer(isize),
    Float(f64),
    String(String),
    Bool(bool),
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
    pub name: String,
    pub value: Constant,
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone)]
pub enum FieldType {
    Double,
    Float,
    Int32,
    Int64,
    Uint32,
    Uint64,
    Sint32,
    Sint64,
    Fixed32,
    Fixed64,
    Sfixed32,
    Sfixed64,
    Bool,
    String,
    Bytes,
    /// Either a message or enum type.
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

    /// Gets an str representing the proto identifier for this type.
    /// This string is used in the name of all wire format functions so can
    /// be used for code generation.
    pub fn as_str(&self) -> &str {
        use self::FieldType::*;
        match self {
            Double => "double",
            Float => "float",
            Int32 => "int32",
            Int64 => "int64",
            Uint32 => "uint32",
            Uint64 => "uint64",
            Sint32 => "sint32",
            Sint64 => "sint64",
            Fixed32 => "fixed32",
            Fixed64 => "fixed64",
            Sfixed32 => "sfixed32",
            Sfixed64 => "sfixed64",
            Bool => "bool",
            String => "string",
            Bytes => "bytes",
            FieldType::Named(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub label: Label,
    pub typ: FieldType,
    pub name: String,
    pub num: FieldNumber,
    pub options: FieldOptions,
    pub unknown_options: Vec<Opt>,
}

impl Field {
    fn to_proto(&self, oneof_index: Option<usize>) -> pb::FieldDescriptorProto {
        let mut proto = pb::FieldDescriptorProto::default();
        proto.set_name(&self.name);
        proto.set_number(self.num as i32);
        proto.set_label(self.label.to_proto());

        match &self.typ {
            FieldType::Double => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_DOUBLE),
            FieldType::Float => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_FLOAT),
            FieldType::Int32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_INT32),
            FieldType::Int64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_INT64),
            FieldType::Uint32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_UINT32),
            FieldType::Uint64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_UINT64),
            FieldType::Sint32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_SINT32),
            FieldType::Sint64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_SINT64),
            FieldType::Fixed32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_FIXED32),
            FieldType::Fixed64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_FIXED64),
            FieldType::Sfixed32 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_SFIXED32),
            FieldType::Sfixed64 => proto.set_typ(pb::FieldDescriptorProto_Type::TYPE_SFIXED64),
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

        // TODO: options

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

#[derive(Debug, Clone, Default)]
pub struct FieldOptions {
    // TODO: Will be true by default in proto3 for any scalar type.
    // Basically anything with a known length
    pub packed: bool,
    pub deprecated: bool,
    pub default: Option<Constant>,
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
                EnumBodyItem::Option(_) => todo!(),
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
    fn to_proto(&self) -> protobuf_descriptor::DescriptorProto {
        let mut proto = protobuf_descriptor::DescriptorProto::default();
        proto.set_name(&self.name);
        for item in &self.body {
            match item {
                MessageItem::Field(f) => {
                    proto.add_field(f.to_proto(None));
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
                        proto.add_field(field.to_proto(Some(idx)));
                    }
                }
                MessageItem::Message(m) => {
                    proto.add_nested_type(m.to_proto());
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
                            options: FieldOptions::default(),
                            unknown_options: vec![],
                        }
                        .to_proto(None),
                    );
                    entry.add_field(
                        Field {
                            label: Label::Optional,
                            typ: f.value_type.clone(),
                            name: "value".to_string(),
                            num: 2, // TODO: Define this in some constants file
                            options: FieldOptions::default(),
                            unknown_options: vec![],
                        }
                        .to_proto(None),
                    );

                    proto.add_field(
                        Field {
                            label: Label::Repeated,
                            typ: FieldType::Named(entry.name().to_string()),
                            name: f.name.to_string(),
                            num: f.num,
                            options: FieldOptions::default(),
                            unknown_options: vec![],
                        }
                        .to_proto(None),
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
                MessageItem::Option(v) => {
                    // TODO
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
    Group(Group),
}

#[derive(Debug, Clone)]
pub struct Extend {
    pub typ: String,
    pub body: Vec<ExtendItem>,
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
                _ => todo!(),
            }
        }

        proto
    }

    pub fn rpcs(&self) -> impl Iterator<Item = &RPC> {
        self.body.iter().filter_map(|item| {
            if let ServiceItem::RPC(r) = item {
                Some(r)
            } else {
                None
            }
        })
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

        // TODO: Options

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
    pub options: Vec<Opt>,
    pub definitions: Vec<TopLevelDef>,
}

impl Proto {
    pub fn to_proto(&self) -> protobuf_descriptor::FileDescriptorProto {
        let mut proto = protobuf_descriptor::FileDescriptorProto::default();
        proto.set_syntax(match self.syntax {
            Syntax::Proto2 => "proto2",
            Syntax::Proto3 => "proto3",
        });

        proto.set_package(&self.package);
        // for import in &self.imports {
        //     proto.add_dependency(v)
        // }

        for def in &self.definitions {
            match def {
                TopLevelDef::Message(m) => {
                    proto.add_message_type(m.to_proto());
                }
                TopLevelDef::Enum(e) => {
                    proto.add_enum_type(e.to_proto());
                }
                TopLevelDef::Extend(_) => todo!(),
                TopLevelDef::Service(s) => {
                    proto.add_service(s.to_proto());
                }
                TopLevelDef::Option(_) => todo!(),
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
