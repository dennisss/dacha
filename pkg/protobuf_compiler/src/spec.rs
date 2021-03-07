// This file contains structs which define the syntax tree of a .proto file
// describing a set of messages/services.

/// In range [1, 2^29 - 1] except [19000, 19999] is reserved.
pub type FieldNumber = u32;

pub type ExtensionNumberType = FieldNumber;

/// Type used in memory to store the value of an enum field.
/// NOTE: Can be negative.
pub type EnumValue = i32;

// Proto 2 and 3
#[derive(Clone, Debug)]
pub enum Constant {
    Identifier(String),
    Integer(isize),
    Float(f64),
    String(String),
    Bool(bool),
}

#[derive(PartialEq, Debug, Clone)]
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
    Fields(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub name: String,
    pub body: Vec<EnumBodyItem>,
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
pub struct Message {
    pub name: String,
    pub body: Vec<MessageItem>,
}

impl Message {
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
    Message(Message),
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

#[derive(Debug, Clone)]
pub struct RPC {
    pub name: String,
    pub req_type: String,
    pub req_stream: bool,
    pub res_type: String,
    pub res_stream: bool,
    pub options: Vec<Opt>,
}

#[derive(Debug, Clone)]
pub struct Stream {
    pub name: String,
    pub input_type: String,
    pub output_type: String,
    pub options: Vec<Opt>,
}

#[derive(Debug, Clone)]
pub struct Proto {
    pub syntax: Syntax,
    pub package: String,
    pub imports: Vec<Import>,
    pub options: Vec<Opt>,
    pub definitions: Vec<TopLevelDef>,
}

#[derive(Debug, Clone)]
pub enum TopLevelDef {
    Message(Message),
    Enum(Enum),
    Extend(Extend),
    Service(Service),
}

pub struct Positioned {}
