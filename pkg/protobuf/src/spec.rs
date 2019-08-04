

// Proto 2 and 3
#[derive(Clone, Debug)]
pub enum Constant {
	Identifier(String),
	Integer(isize),
	Float(f64),
	String(String),
	Bool(bool)
}

#[derive(Debug, Clone)]
pub enum Syntax {
	Proto2,
	Proto3
}

#[derive(Debug, Clone)]
pub enum ImportType {
	Default,
	Weak,
	Public
}

#[derive(Debug, Clone)]
pub struct Import {
	pub typ: ImportType,
	pub path: String
}

#[derive(Clone, Debug)]
pub struct Opt {
	pub name: String,
	pub value: Constant
}

#[derive(Debug, Clone, PartialEq)]
pub enum Label {
	Required,
	Optional,
	Repeated
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
	// Either a message or enum type.
	Named(String)
}

impl FieldType {
	// Gets an str representing the proto identifier for this type
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
			FieldType::Named(s) => s.as_str()
		}
	}
}


#[derive(Debug, Clone)]
pub struct Field {
	pub label: Label,
	pub typ: FieldType,
	pub name: String,
	pub num: usize,
	pub options: Vec<Opt>
}

// Proto 2
#[derive(Debug, Clone)]
pub struct Group {
	pub label: Label,
	pub name: String,
	pub num: usize,
	pub body: Vec<MessageItem>
}

#[derive(Debug, Clone)]
pub struct OneOf {
	pub name: String,
	pub fields: Vec<OneOfField>
}

#[derive(Debug, Clone)]
pub struct OneOfField {
	pub typ: FieldType,
	pub name: String,
	pub num: usize,
	pub options: Vec<Opt>
}

#[derive(Debug, Clone)]
pub struct MapField {
	pub key_type: FieldType,
	pub value_type: FieldType,
	pub name: String,
	pub num: usize,
	pub options: Vec<Opt>
}

pub type Ranges = Vec<Range>;

// Upper and lower bounds are inclusive.
pub type Range = (usize, usize);

#[derive(Debug, Clone)]
pub enum Reserved {
	Ranges(Ranges),
	Fields(Vec<String>)
}

#[derive(Debug, Clone)]
pub struct Enum {
	pub name: String,
	pub body: Vec<EnumBodyItem>
}

#[derive(Debug, Clone)]
pub enum EnumBodyItem {
	Option(Opt),
	Field(EnumField)
}

#[derive(Debug, Clone)]
pub struct EnumField {
	pub name: String,
	pub num: usize,
	pub options: Vec<Opt>
}

#[derive(Debug, Clone)]
pub struct Message {
	pub name: String,
	pub body: Vec<MessageItem>
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
	Reserved(Reserved)
}

#[derive(Debug, Clone)]
pub enum ExtendItem {
	Field(Field),
	Group(Group)
}

#[derive(Debug, Clone)]
pub struct Extend {
	pub typ: String,
	pub body: Vec<ExtendItem>
}

#[derive(Debug, Clone)]
pub enum ServiceItem {
	Option(Opt),
	RPC(RPC),
	Stream(Stream)
}

#[derive(Debug, Clone)]
pub struct Service {
	pub name: String,
	pub body: Vec<ServiceItem>
}

#[derive(Debug, Clone)]
pub struct RPC {
	pub name: String,
	pub req_type: String,
	pub req_stream: bool,
	pub res_type: String,
	pub res_stream: bool,
	pub options: Vec<Opt>
}

#[derive(Debug, Clone)]
pub struct Stream {
	pub name: String,
	pub input_type: String,
	pub output_type: String,
	pub options: Vec<Opt>
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
	Service(Service)
}


