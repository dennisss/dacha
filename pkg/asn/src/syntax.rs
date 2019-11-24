use common::errors::*;
use super::tokenizer::Token;
use parsing::*;
use bytes::Bytes;
use parsing::ascii::AsciiString;
use common::errors::Error;
use std::convert::AsRef;
use common::bits::BitVector;
use std::string::ToString;
use super::tag::TagClass;

parser!(number<usize> => Token::skip_to(Token::number));
parser!(realnumber<f64> => Token::skip_to(Token::realnumber));
parser!(identifier<AsciiString> => Token::skip_to(Token::identifier));

fn symbol(c: char) -> impl Parser<()> {
	and_then(Token::skip_to(Token::symbol), move |s| {
		if s != c as u8 {
			return Err("Wrong symbol".into());
		}

		Ok(())
	})
}

fn reserved(w: &'static str) -> impl Parser<()> {
	// TODO: Finish this
	and_then(Token::skip_to(Token::reserved), move |s| {
		if s.as_ref() != w {
			return Err(format!("Wrong reserved: '{}'", w).into());
		}

		Ok(())
	})
}

fn sequence(w: &'static str) -> impl Parser<()> {
	// TODO: Finish this
	and_then(Token::skip_to(Token::sequence), move |s| {
		if s.as_ref() != w {
			return Err("Wrong sequence".into());
		}

		Ok(())
	})
}

// TODO: Check this
parser!(psname<AsciiString> => typereference);

parser!(valuereference<AsciiString> => identifier);
parser!(bstring<BitVector> => Token::skip_to(Token::bstring));
parser!(hstring<Bytes> => Token::skip_to(Token::hstring));
parser!(typereference<AsciiString> => Token::skip_to(Token::typereference));
parser!(modulereference<AsciiString> => typereference);

parser!(encodingreference<AsciiString> => and_then(typereference, |s| {
	for c in s.as_ref().chars() {
		if c.is_ascii_lowercase() {
			return Err("Must be all upper case".into());
		}
	}

	Ok(s)
}));

/*
ModuleDefinition ::=
	ModuleIdentifier
	DEFINITIONS
	EncodingReferenceDefault
	TagDefault
	ExtensionDefault
	"::="
	BEGIN
	ModuleBody
	EncodingControlSections
	END
*/
#[derive(Debug)]
pub struct ModuleDefinition {
	pub ident: ModuleIdentifier,
	pub default_mode: Option<TagDefault>,
	/// Whether or not all extensible types by default allow extensibility after
	/// the last field.
	pub extension_default: bool,
	pub body: Option<ModuleBody>
}

impl ModuleDefinition {
	parser!(pub parse<Self> => seq!(c => {
		let ident = c.next(ModuleIdentifier::parse)?;
		c.next(reserved("DEFINITIONS"))?;

		let default_mode = c.next(TagDefault::parse)?;
		let extension_default = c.next(ExtensionDefault::parse)?.is_some();
		c.next(sequence("::="))?;
		c.next(reserved("BEGIN"))?;
		let body = c.next(ModuleBody::parse)?;
		c.next(reserved("END"))?;

		// Skip all trailing comments/whitespace in the file.
		c.next(Token::skip_to(|i| Ok(((), i))))?;

		Ok(Self { ident, default_mode, extension_default, body })
	}));
}


/* ModuleIdentifier ::= modulereference DefinitiveIdentification */
#[derive(Debug)]
pub struct ModuleIdentifier {
	pub name: AsciiString,

	/// NOTE: If specified, then this can't reference any values defined in the
	/// module.
	pub oid: Option<ObjectIdentifierValue>
}

impl ModuleIdentifier {
	parser!(parse<Self> => seq!(c => {
		let name = c.next(modulereference)?;
		let oid = c.next(opt(ObjectIdentifierValue::parse))?;
		Ok(Self { name, oid })
	}));
}

/* DefinitiveIdentification ::= | DefinitiveOID | DefinitiveOIDandIRI | empty */

/* DefinitiveOIDandIRI ::= DefinitiveOID IRIValue */


#[derive(Debug)]
pub struct ObjectIdentifierValue {
	pub components: Vec<ObjectIdentifierComponent>
}

impl ObjectIdentifierValue {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol('{'))?;
		let components = c.next(many1(ObjectIdentifierComponent::parse))?;
		c.next(symbol('}'))?;
		Ok(Self { components })
	}));
}

#[derive(Debug)]
pub enum ObjectIdentifierComponent {
	Name(AsciiString),
	Number(usize),
	NameAndNumber(AsciiString, usize)
}

impl ObjectIdentifierComponent {
	parser!(parse<Self> => alt!(
		seq!(c => {
			let name = c.next(identifier)?;
			let number = c.next(opt(seq!(c => {
				c.next(symbol('('))?;
				let n = c.next(number)?;
				c.next(symbol(')'))?;
				Ok(n)
			})))?;

			Ok(if let Some(n) = number {
				Self::NameAndNumber(name, n)
			} else {
				Self::Name(name)
			})
		}),
		map(number, |v| Self::Number(v))
	));
}



/*
EncodingReferenceDefault ::=
	encodingreference INSTRUCTIONS
	| empty
*/

/*
TagDefault ::=
	EXPLICIT TAGS
	| IMPLICIT TAGS
	| AUTOMATIC TAGS
	| empty
*/
#[derive(Debug, Clone)]
pub enum TagDefault {
	Explicit,
	Implicit,
	Automatic
}

impl TagDefault {
	parser!(parse<Option<Self>> => {
		opt(seq!(c => {
			let v = c.next(alt!(
				map(reserved("EXPLICIT"), |_| Self::Explicit),
				map(reserved("IMPLICIT"), |_| Self::Implicit),
				map(reserved("AUTOMATIC"), |_| Self::Automatic)
			))?;

			c.next(reserved("TAGS"))?;
			Ok(v)
		}))
	});
}


/* ExtensionDefault ::= EXTENSIBILITY IMPLIED | empty */
#[derive(Debug)]
pub struct ExtensionDefault {}

impl ExtensionDefault {
	parser!(parse<Option<Self>> => {
		opt(seq!(c => {
			c.next(reserved("EXTENSIBILITY"))?;
			c.next(reserved("IMPLIED"))?;
			Ok(Self {})
		}))
	});
}


/* ModuleBody ::= Exports Imports AssignmentList | empty */
#[derive(Debug)]
pub struct ModuleBody {
	pub exports: Option<Exports>,
	pub imports: Vec<SymbolsFromModule>,
	pub assignments: Vec<Assignment>
}

impl ModuleBody {
	parser!(parse<Option<Self>> => opt(seq!(c => {
		let exports = c.next(Exports::parse)?;
		let imports = c.next(Imports::parse)?.map(|v| v.symbols).unwrap_or(vec![]);
		let assignments = c.next(AssignmentList::parse)?.0;
		Ok(Self { exports, imports, assignments })
	})));
}


/*
Exports ::=
	EXPORTS SymbolsExported ";"
	| EXPORTS ALL ";"
	| empty
*/
#[derive(Debug)]
pub enum Exports {
	All,
	Symbols(Vec<Symbol>)
}

impl Exports {
	parser!(parse<Option<Self>> => {
		opt(alt!(
			seq!(c => {
				c.next(reserved("EXPORTS"))?;
				let syms = c.next(SymbolsExported::parse)?
					.unwrap_or(SymbolsExported(vec![]));
				c.next(symbol(';'))?;
				Ok(Self::Symbols(syms.0))
			}),
			seq!(c => {
				c.next(reserved("EXPORTS"))?;
				c.next(reserved("ALL"))?;
				c.next(symbol(';'))?;
				Ok(Self::All)
			})
		))
	});
}


/* SymbolsExported ::= SymbolList | empty */
#[derive(Debug)]
pub struct SymbolsExported(Vec<Symbol>);

impl SymbolsExported {
	parser!(parse<Option<Self>> => {
		opt(map(SymbolList::parse, |v| Self(v.0)))
	});
}


/* Imports ::= IMPORTS SymbolsImported ";" | empty */
#[derive(Debug)]
pub struct Imports {
	pub symbols: Vec<SymbolsFromModule>
}

impl Imports {
	parser!(parse<Option<Self>> => opt(seq!(c => {
		c.next(reserved("IMPORTS"))?;
		let symbols = c.next(SymbolsImported::parse)?
			.map(|v| v.0).unwrap_or(vec![]);
		c.next(symbol(';'))?;
		Ok(Self { symbols })
	})));
}


/* SymbolsImported ::= SymbolsFromModuleList | empty */
struct SymbolsImported(Vec<SymbolsFromModule>);

impl SymbolsImported {
	parser!(parse<Option<Self>> => {
		opt(map(SymbolsFromModuleList::parse, |v| Self(v.0)))
	});
}


/*
SymbolsFromModuleList ::=
	SymbolsFromModule
	| SymbolsFromModuleList SymbolsFromModule
*/
struct SymbolsFromModuleList(Vec<SymbolsFromModule>);

impl SymbolsFromModuleList {
	parser!(parse<Self> => {
		map(many1(SymbolsFromModule::parse), |v| Self(v))
	});
}


/* SymbolsFromModule ::= SymbolList FROM GlobalModuleReference */
#[derive(Debug)]
pub struct SymbolsFromModule {
	pub symbols: Vec<Symbol>,
	pub module: GlobalModuleReference
}

impl SymbolsFromModule {
	parser!(parse<Self> => seq!(c => {
		let symbols = c.next(SymbolList::parse)?.0;
		c.next(reserved("FROM"))?;
		let module = c.next(GlobalModuleReference::parse)?;
		Ok(Self { symbols, module })
	}));
}


/* GlobalModuleReference ::= modulereference AssignedIdentifier */
#[derive(Debug)]
pub struct GlobalModuleReference {
	pub name: AsciiString,
	pub assigned_ident: Option<AssignedIdentifier>
}

impl GlobalModuleReference {
	parser!(parse<Self> => seq!(c => {
		let name = c.next(modulereference)?;
		let assigned_ident = c.next(AssignedIdentifier::parse)?;
		Ok(Self { name, assigned_ident })
	}));
}


/*
AssignedIdentifier ::=
	ObjectIdentifierValue
	| DefinedValue
	| empty
*/
#[derive(Debug)]
pub enum AssignedIdentifier {
	ObjectIdentifier(ObjectIdentifierValue),
	Defined(DefinedValue)
}

impl AssignedIdentifier {
	parser!(parse<Option<Self>> => opt(alt!(
		map(ObjectIdentifierValue::parse, |v| Self::ObjectIdentifier(v)),
		map(DefinedValue::parse, |v| Self::Defined(v))
	)));
}


/* SymbolList ::= Symbol | SymbolList "," Symbol */
#[derive(Debug)]
pub struct SymbolList(Vec<Symbol>);

impl SymbolList {
	parser!(parse<Self> => map(delimited1(Symbol::parse, symbol(',')),
							   |v| Self(v)));
}


/* Symbol ::= Reference | ParameterizedReference */
/* ParameterizedReference ::= Reference | Reference "{" "}" */
#[derive(Debug)]
pub struct Symbol {
	pub reference: Reference,
	pub parameterized: bool
}

impl Symbol {
	parser!(parse<Self> => seq!(c => {
		let reference = c.next(Reference::parse)?;
		let parameterized = c.next(opt(seq!(c => {
			c.next(symbol('{'))?;
			c.next(symbol('}'))?;
			Ok(())
		})))?.is_some();

		Ok(Self { reference, parameterized })
	}));
}


/*
Reference ::=
	typereference
	| valuereference
	| objectclassreference
	| objectreference
	| objectsetreference
*/
#[derive(Debug)]
pub enum Reference {
	Type(AsciiString),
	Value(AsciiString)
}

impl Reference {
	parser!(parse<Self> => alt!(
		map(typereference, |r| Self::Type(r)),
		map(valuereference, |r| Self::Value(r))
	));
}


#[derive(Debug)]
pub struct AssignmentList(Vec<Assignment>);

impl AssignmentList {
	parser!(parse<Self> => map(many1(Assignment::parse), |v| Self(v)));
}


/*
Assignment ::=
	TypeAssignment
	| ValueAssignment
	| XMLValueAssignment
	| ValueSetTypeAssignment
	| ObjectClassAssignment
	| ObjectAssignment
	| ObjectSetAssignment
	| ParameterizedAssignment
*/
#[derive(Debug)]
pub enum Assignment {
	Type(TypeAssignment),
	Value(ValueAssignment)
}

impl Assignment {
	parser!(parse<Self> => alt!(
		map(TypeAssignment::parse, |v| Self::Type(v)),
		map(ValueAssignment::parse, |v| Self::Value(v))
	));
}


/*
DefinedType ::=
	ExternalTypeReference
	| typereference
	| ParameterizedType
	| ParameterizedValueSetType
*/

/*
DefinedValue ::=
	ExternalValueReference
	| valuereference
	| ParameterizedValue
*/
#[derive(Debug)]
pub struct DefinedValue(pub AsciiString);

impl DefinedValue {
	parser!(parse<Self> => alt!(
		map(ExternalValueReference::parse, |v| Self(v.0)),
		map(valuereference, |v| Self(v))
		// TODO
	));
}


/* ExternalTypeReference ::= modulereference "." typereference */
#[derive(Debug)]
pub struct ExternalTypeReference(AsciiString);

impl ExternalTypeReference {
	parser!(parse<Self> => {
		seq!(c => {
			let module = c.next(modulereference)?;
			c.next(symbol('.'))?;
			let name = c.next(typereference)?;
			Ok(Self(AsciiString::from_string(
				format!("{}.{}", module.as_ref(), name.as_ref())).unwrap()))
		})
	});
}


/* ExternalValueReference ::= modulereference "." valuereference */
#[derive(Debug)]
pub struct ExternalValueReference(AsciiString);

impl ExternalValueReference {
	parser!(parse<Self> => {
		seq!(c => {
			let module = c.next(modulereference)?;
			c.next(symbol('.'))?;
			let name = c.next(valuereference)?;
			Ok(Self(AsciiString::from_string(
				format!("{}.{}", module.as_ref(), name.as_ref())).unwrap()))
		})
	});
}

// NOTE: Rc is mainly convenient for the compiler.
use std::rc::Rc;

/* TypeAssignment ::= typereference "::=" Type */
#[derive(Debug)]
pub struct TypeAssignment {
	pub name: AsciiString,
	pub typ: Type
}

impl TypeAssignment {
	parser!(parse<Self> => seq!(c => {
		let name = c.next(typereference)?;
		c.next(sequence("::="))?;
		let typ = c.next(Type::parse)?;
		Ok(Self { name, typ })
	}));
}


/* ValueAssignment ::= valuereference Type "::=" Value */
#[derive(Debug)]
pub struct ValueAssignment {
	pub name: AsciiString,
	pub typ: Type,
	pub value: Rc<Value>
}

impl ValueAssignment {
	parser!(parse<Self> => seq!(c => {
		let name = c.next(valuereference)?;
		let typ = c.next(Type::parse)?;
		c.next(sequence("::="))?;
		let value = Rc::new(c.next(Value::parse)?);
		Ok(Self { name, typ, value })
	}));
}


/*
ValueSetTypeAssignment ::=
	typereference
	Type
	"::="
	ValueSet
*/

/* ValueSet ::= "{" ElementSetSpecs "}" */

/* Type ::= BuiltinType | ReferencedType | ConstrainedType */
#[derive(Debug)]
pub struct Type {
	pub prefixes: Vec<TypePrefix>,
	pub desc: TypeDesc,
	pub constraints: Vec<Constraint>
}

#[derive(Debug)]
pub enum TypeDesc {
	Builtin(Rc<BuiltinType>),
	Referenced(AsciiString)
}

impl Type {
	parser!(parse<Self> => seq!(c => {
		let prefixes = c.next(many(TypePrefix::parse))?;
		let desc = c.next(alt!(
			map(BuiltinType::parse, |v| TypeDesc::Builtin(Rc::new(v))),
			map(ReferencedType::parse, |v| TypeDesc::Referenced(v.0))	
		))?;
		let constraints = c.next(many(Constraint::parse))?;
		Ok(Self { prefixes, desc, constraints })
	}));
}


#[derive(Debug)]
pub enum BuiltinType {
	Any(AnyType),
	BitString(BitStringType),
	Boolean,
	CharacterString(CharacterStringType),
	Choice(ChoiceType),
	Date,
	DateTime,
	Duration,
	EmbeddedPDVType,
	Enumerated(EnumeratedType),
	ExternalType,
	InstanceOfType,
	Integer(IntegerType),
	IRIType,
	Null,
	ObjectClassFieldType,
	ObjectIdentifier,
	OctetString,
	Real,
	RelativeIRIType,
	RelativeOIDType,
	Sequence(ComponentBody),
	SequenceOf(CollectionType),
	Set(ComponentBody),
	SetOf(CollectionType),
	Time,
	TimeOfDay
}

impl BuiltinType {
	parser!(parse<Self> => alt!(
		map(AnyType::parse, |v| Self::Any(v)),
		map(BitStringType::parse, |v| Self::BitString(v)),
		map(BooleanType::parse, |v| Self::Boolean),
		map(CharacterStringType::parse, |v| Self::CharacterString(v)),
		map(ChoiceType::parse, |v| Self::Choice(v)),
		map(reserved("DATE"), |_| BuiltinType::Date),
		map(reserved("DATE-TIME"), |_| BuiltinType::DateTime),
		map(reserved("DURATION"), |_| BuiltinType::Duration),
		map(EmbeddedPDVType::parse, |_| BuiltinType::EmbeddedPDVType),
		map(EnumeratedType::parse, |v| BuiltinType::Enumerated(v)),

		map(IntegerType::parse, |v| Self::Integer(v)),

		map(reserved("NULL"), |_| BuiltinType::Null),

		map(ObjectIdentifierType::parse, |_| BuiltinType::ObjectIdentifier),
		map(OctetStringType::parse, |_| BuiltinType::OctetString),
		map(reserved("REAL"), |_| BuiltinType::Real),

		map(reserved("RELATIVE-OID"), |_| BuiltinType::RelativeOIDType),
		map(SequenceType::parse, |v| Self::Sequence(v.0)),
		// TODO: Export the constraints
		map(SequenceOfType::parse, |(_, v)| Self::SequenceOf(v.0)),
		map(SetType::parse, |v| Self::Set(v.0)),
		map(SetOfType::parse, |(_, v)| Self::SetOf(v.0)),

		map(reserved("TIME"), |_| Self::Time),
		map(reserved("TIME-OF-DAY"), |_| Self::TimeOfDay)
	));
}


/*
ReferencedType ::=
	DefinedType
	| UsefulType
	| SelectionType
	| TypeFromObject
	| ValueSetFromObjects
*/
#[derive(Debug)]
pub struct ReferencedType(AsciiString);

impl ReferencedType {
	parser!(parse<Self> => map(UsefulType::parse, |v| Self(v.0)));
}


#[derive(Debug)]
pub struct NamedType {
	pub name: AsciiString,
	pub typ: Type
}

impl NamedType {
	parser!(parse<Self> => seq!(c => {
		let name = c.next(identifier)?;
		let typ = c.next(Type::parse)?;
		Ok(Self { name, typ })
	}));
}


#[derive(Debug)]
pub enum Value {
	Builtin(Rc<BuiltinValue>),
	Referenced(ReferencedValue),
	// ObjectClassField(ObjectClassFieldValue)
}

impl Value {
	parser!(parse<Self> => alt!(
		// TODO
		map(BuiltinValue::parse, |v| Self::Builtin(Rc::new(v))),
		map(ReferencedValue::parse, |v| Self::Referenced(v))
		// map(ObjectClassFieldValue::parse, |v| Self::ObjectClassField(v))
	));
}

#[derive(Debug)]
pub enum BuiltinValue {
	BitStringValue,
	Boolean(bool),
	CharacterString(Bytes),
	ChoiceValue,
	EmbeddedPDVValue,
	EnumeratedValue,
	ExternalValue,
	InstanceOfValue,
	Integer(IntegerValue),
	IRIValue, // TODO: Will be parsed from a CharacterString
	Null,
	/// TODO: Ensure that the compiler builds the value in the context of the
	/// type assigned to it.
	/// 
	/// NOTE: If this is a relative oid, any Name form components in this must
	/// be a reference (as we can't resolve a full path in the registry using
	/// a relative value).
	ObjectIdentifier(ObjectIdentifierValue),
	OctetString(OctetStringValue),
	RealValue,
	RelativeIRIValue, // TODO: Will be parsed from a CharacterString
	Sequence(SequenceValue),
	SequenceOfValue,
	SetValue,
	SetOfValue,
	TimeValue /* TimeValue ::= tstring  */
}

impl BuiltinValue {
	parser!(parse<Self> => {
		// TODO
		alt!(
			map(BooleanValue::parse, |v| Self::Boolean(v.0)),
			map(Token::skip_to(Token::parse_string), |v| {
				Self::CharacterString(v)
			}),
			map(IntegerValue::parse, |v| Self::Integer(v)),
			map(reserved("NULL"), |_| Self::Null),
			map(ObjectIdentifierValue::parse, |v| Self::ObjectIdentifier(v)),
			map(OctetStringValue::parse, |v| Self::OctetString(v)),
			// TODO: THis is difficult to differentiate w.r.t. ObjectIdentifier.
			map(SequenceValue::parse, |v| Self::Sequence(v))
		)
	});
}

#[derive(Debug)]
pub struct AnyType {
	/// If specified, then this will be the name of a field containing an
	/// oid that identifies the type. 
	pub defined_by: Option<AsciiString>
}

impl AnyType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("ANY"))?;
		let defined_by = c.next(opt(seq!(c => {
			c.next(reserved("DEFINED"))?;
			c.next(reserved("BY"))?;
			c.next(identifier)
		})))?;

		Ok(Self { defined_by })
	}));
}



/* ReferencedValue ::= DefinedValue | ValueFromObject */
#[derive(Debug)]
pub struct ReferencedValue(pub DefinedValue);

impl ReferencedValue {
	parser!(parse<Self> => map(DefinedValue::parse, |v| Self(v)));
}


/* NamedValue ::= identifier Value */
#[derive(Debug)]
pub struct NamedValue {
	pub name: AsciiString,
	pub value: Value
}

impl NamedValue {
	parser!(parse<Self> => seq!(c => {
		let name = c.next(identifier)?;
		let value = c.next(Value::parse)?;
		Ok(Self { name, value })
	}));
}


/* BooleanType ::= BOOLEAN */
#[derive(Debug)]
pub struct BooleanType {}

impl BooleanType {
	parser!(parse<()> => reserved("BOOLEAN"));
}

/* BooleanValue ::= TRUE | FALSE */
pub struct BooleanValue(bool);

impl BooleanValue {
	parser!(parse<Self> => alt!(
		map(reserved("TRUE"), |_| Self(true)),
		map(reserved("FALSE"), |_| Self(false))
	));
}


/* IntegerType ::= INTEGER | INTEGER "{" NamedNumberList "}" */
#[derive(Debug)]
pub struct IntegerType {
	/// If specified, is an enumeration of all allowed values.
	pub values: Option<Vec<NamedNumber>>
}

impl IntegerType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("INTEGER"))?;
		let values = c.next(opt(seq!(c => {
			c.next(symbol('{'))?;
			let list = c.next(NamedNumberList::parse)?;
			c.next(symbol('}'))?;
			Ok(list.0)
		})))?;
		Ok(Self { values })
	}));
}


/* NamedNumberList ::= NamedNumber | NamedNumberList "," NamedNumber */
#[derive(Debug)]
pub struct NamedNumberList(Vec<NamedNumber>);

impl NamedNumberList {
	parser!(parse<Self> => {
		map(delimited1(NamedNumber::parse, symbol(',')), |arr| Self(arr))
	});
}


/*
NamedNumber ::=
	identifier "(" SignedNumber ")"
	| identifier "(" DefinedValue ")"
*/
#[derive(Debug)]
pub struct NamedNumber {
	pub name: AsciiString,
	pub value: NamedNumberValue
}

#[derive(Debug)]
pub enum NamedNumberValue {
	Immediate(isize),
	Defined(AsciiString)
}

impl NamedNumber {
	parser!(parse<Self> => seq!(c => {
		let name = c.next(identifier)?;
		c.next(symbol('('))?;
		let value = c.next(alt!(
			map(SignedNumber::parse, |v| NamedNumberValue::Immediate(v.0)),
			map(DefinedValue::parse, |v| NamedNumberValue::Defined(v.0))
		))?;
		c.next(symbol(')'))?;
		Ok(Self { name, value })
	}));
}


/* SignedNumber ::= number | "-" number */
#[derive(Debug)]
pub struct SignedNumber(isize);

impl SignedNumber {
	parser!(parse<Self> => {
		seq!(c => {
			let neg = c.next(opt(symbol('-')))?.is_some();
			let mut num = c.next(number)? as isize;
			if neg {
				num *= -1;
			}

			Ok(Self(num))
		})
	});
}


/* IntegerValue ::= SignedNumber | identifier */
#[derive(Debug)]
pub enum IntegerValue {
	SignedNumber(isize),
	Identifier(AsciiString)
}

impl IntegerValue {
	parser!(parse<Self> => alt!(
		map(SignedNumber::parse, |v| Self::SignedNumber(v.0)),
		map(identifier, |v| Self::Identifier(v))
	));
}


/* TextInteger ::= identifier */
#[derive(Debug)]
pub struct TextInteger(AsciiString);

impl TextInteger {
	parser!(parse<Self> => map(identifier, |v| Self(v)));
}


/* EnumeratedType ::= ENUMERATED "{" Enumerations "}" */
#[derive(Debug)]
pub struct EnumeratedType(pub Enumerations);

impl EnumeratedType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("ENUMERATED"))?;
		c.next(symbol('{'))?;
		let val = c.next(Enumerations::parse)?;
		c.next(symbol('}'))?;
		Ok(Self(val))
	}));
}


/*
Enumerations ::=
	RootEnumeration
	| RootEnumeration "," "..." ExceptionSpec
	| RootEnumeration "," "..." ExceptionSpec "," AdditionalEnumeration
*/
#[derive(Debug)]
pub struct Enumerations {
	pub items: Vec<NamedNumber>,
	pub extensible: bool,
	pub exception: Option<ExceptionSpec>
}

impl Enumerations {
	parser!(parse<Self> => seq!(c => {
		let mut items_raw = c.next(RootEnumeration::parse)?.0;
		let vals = c.next(opt(seq!(c => {
			c.next(symbol(','))?;
			c.next(sequence("..."))?;
			let exception = c.next(ExceptionSpec::parse)?;

			let additional = c.next(opt(seq!(c => {
				c.next(symbol(','))?;
				c.next(map(AdditionalEnumeration::parse, |v| v.0))
			})))?.unwrap_or(vec![]);

			Ok((exception, additional))
		})))?;

		let (extensible, exception) =
			if let Some((exception, mut addition)) = vals {
				items_raw.append(&mut addition);
				(true, exception)
			} else {
				(false, None)
			};

		let mut items = vec![];
		for (i, item) in items_raw.into_iter().enumerate() {
			// TODO: If an un-numbered item appears after a numbered one, we
			// should start numbered based on that one (although this is
			// complicated by referenced numbers).
			items.push(NamedNumber {
				name: item.name,
				value: item.number
					.unwrap_or(NamedNumberValue::Immediate(i as isize))
			});
		}

		Ok(Self { items, extensible, exception })
	}));
}


/* RootEnumeration ::= Enumeration */
struct RootEnumeration {}

impl RootEnumeration {
	parser!(parse<Enumeration> => Enumeration::parse);
}


/* AdditionalEnumeration ::= Enumeration */
struct AdditionalEnumeration {}

impl AdditionalEnumeration {
	parser!(parse<Enumeration> => Enumeration::parse);
}


/* Enumeration ::= EnumerationItem | EnumerationItem "," Enumeration */
struct Enumeration(Vec<EnumerationItem>);

impl Enumeration {
	parser!(parse<Self> => map(
		delimited1(EnumerationItem::parse, symbol(',')),
		|v| Self(v)));
}


/* EnumerationItem ::= identifier | NamedNumber */
struct EnumerationItem {
	name: AsciiString,
	number: Option<NamedNumberValue>
}

impl EnumerationItem {
	parser!(parse<Self> => alt!(
		map(NamedNumber::parse,
			|v| Self { name: v.name, number: Some(v.value) }),
		// NOTE: This must be after NamedNumber to parse correctly.
		map(identifier, |name| Self { name, number: None })
	));
}


/* EnumeratedValue ::= identifier */
#[derive(Debug)]
pub struct EnumeratedValue(AsciiString);

impl EnumeratedValue {
	parser!(parse<Self> => map(identifier, |v| Self(v)));
}


/* RealValue ::= NumericRealValue | SpecialRealValue */

/*
NumericRealValue ::=
	realnumber
	| "-" realnumber
	| SequenceValue
*/

/* SpecialRealValue ::= PLUS-INFINITY | MINUS-INFINITY | NOT-A-NUMBER */
#[derive(Debug)]
pub enum SpecialRealValue {
	Infinity,
	NegInfinity,
	NaN
}

impl SpecialRealValue {
	parser!(parse<Self> => alt!(
		map(reserved("PLUS-INFINITY"), |_| Self::Infinity),
		map(reserved("MINUS-INFINITY"), |_| Self::NegInfinity),
		map(reserved("NOT-A-NUMBER"), |_| Self::NaN)
	));
}


/* BitStringType ::= BIT STRING | BIT STRING "{" NamedBitList "}" */
#[derive(Debug)]
pub struct BitStringType {
	pub named_bits: Vec<NamedBit>
}

impl BitStringType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("BIT"))?;
		c.next(reserved("STRING"))?;

		let named_bits = c.next(opt(seq!(c => {
			c.next(symbol('{'))?;
			let list = c.next(delimited1(NamedBit::parse, symbol(',')))?;
			c.next(symbol('}'))?;
			Ok(list)
		})))?.unwrap_or(vec![]);

		Ok(Self { named_bits })
	}));
}



/* NamedBit ::= identifier "(" number ")" | identifier "(" DefinedValue ")" */
/* NamedBitList ::= NamedBit | NamedBitList "," NamedBit */
#[derive(Debug)]
pub struct NamedBit {
	pub name: AsciiString,
	pub value: NamedBitValue
}

#[derive(Debug)]
pub enum NamedBitValue {
	Immediate(usize),
	Defined(AsciiString)
}

impl NamedBit {
	parser!(parse<Self> => seq!(c => {
		let name = c.next(identifier)?;
		c.next(symbol('('))?;
		let value = c.next(alt!(
			map(number, |v| NamedBitValue::Immediate(v)),
			map(DefinedValue::parse, |v| NamedBitValue::Defined(v.0))
		))?;
		c.next(symbol(')'))?;
		Ok(Self { name, value })
	}));
}


/*
BitStringValue ::=
	bstring
	| hstring
	| "{" IdentifierList "}"
	| "{" "}"
	| CONTAINING Value
*/

/* IdentifierList ::= identifier | IdentifierList "," identifier */
#[derive(Debug)]
pub struct IdentifierList(Vec<AsciiString>);

impl IdentifierList {
	parser!(parse<Self> => {
		map(delimited1(identifier, symbol(',')), |v| Self(v))
	});
}


/* OctetStringType ::= OCTET STRING */
#[derive(Debug)]
pub struct OctetStringType {}

impl OctetStringType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("OCTET"))?;
		c.next(reserved("STRING"))?;
		Ok(Self {})
	}));
}


// TODO: Can not be distinguished from a BitStringValue without further
// analysis.
/*
OctetStringValue ::=
	bstring
	| hstring
	| CONTAINING Value
*/
#[derive(Debug)]
pub enum OctetStringValue {
	Bits(BitVector),
	Hex(Bytes)
}

impl OctetStringValue {
	parser!(parse<Self> => {
		alt!(
			map(bstring, |v| Self::Bits(v)),
			map(hstring, |v| Self::Hex(v))
		)
	});
}


/*
SequenceType ::=
	SEQUENCE "{" "}"
	| SEQUENCE "{" ExtensionAndException OptionalExtensionMarker "}"
	| SEQUENCE "{" ComponentTypeLists "}"
*/
#[derive(Debug)]
pub struct SequenceType(pub ComponentBody);

impl SequenceType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("SEQUENCE"))?;
		let body = c.next(ComponentBody::parse)?;
		Ok(Self(body))
	}));
}

#[derive(Debug)]
pub struct ComponentBody {
	pub types: Vec<ComponentType>
	
	// TODO: Add extension index and 

	// Empty,
	// ExtensionAndException(ExtensionAndException),
	// ComponentTypes(ComponentTypeLists)
}

impl ComponentBody {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol('{'))?;
		let types = c.next(opt(map(ComponentTypeList::parse, |v| v.0)))?
			.unwrap_or(vec![]);
		c.next(symbol('}'))?;

		/*
		let val = c.next(alt!(
			seq!(c => {
				let v = c.next(ExtensionAndException::parse)?;
				c.next(OptionalExtensionMarker::parse)?;
				c.next(symbol('}'))?;
				Ok(Self::ExtensionAndException(v))
			}),
			seq!(c => {
				let v = c.next(ComponentTypeLists::parse)?;
				c.next(symbol('}'))?;
				Ok(Self::ComponentTypes(v))
			}),
			map(symbol('}'), |_| Self::Empty)
		))?;
		*/
		Ok(Self { types })
	}));
}


/* ExtensionAndException ::= "..." | "..." ExceptionSpec */
#[derive(Debug)]
pub struct ExtensionAndException(Option<ExceptionSpec>);

impl ExtensionAndException {
	parser!(parse<Self> => map(ExceptionSpec::parse, |v| Self(v)));
}


/* OptionalExtensionMarker ::= "," "..." | empty */
#[derive(Debug)]
pub struct OptionalExtensionMarker {}

impl OptionalExtensionMarker {
	parser!(parse<Option<ExtensionEndMarker>> =>
		opt(ExtensionEndMarker::parse));
}


/*
ComponentTypeLists ::=
	RootComponentTypeList
	| RootComponentTypeList "," ExtensionAndException ExtensionAdditions
	  OptionalExtensionMarker
	| RootComponentTypeList "," ExtensionAndException ExtensionAdditions
	  ExtensionEndMarker "," RootComponentTypeList
	| ExtensionAndException ExtensionAdditions ExensionEndMarker ","
	  RootComponentTypeList
	| ExtensionAndException ExtensionAdditions OptionalExtensionMarker
*/

/* RootComponentTypeList ::= ComponentTypeList */

/* ExtensionEndMarker ::= "," "..." */
#[derive(Debug)]
pub struct ExtensionEndMarker {}

impl ExtensionEndMarker {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol(','))?;
		c.next(sequence("..."))?;
		Ok(Self {})
	}));
}


/* ExtensionAdditions ::= "," ExtensionAdditionList | empty */
#[derive(Debug)]
pub struct ExtensionAdditions(Vec<ExtensionAddition>);

impl ExtensionAdditions {
	parser!(parse<Option<Self>> => opt(seq!(c => {
		c.next(symbol(','))?;
		let arr = c.next(ExtensionAdditionList::parse)?.0;
		Ok(Self(arr))
	})));
}


/*
ExtensionAdditionList ::=
	ExtensionAddition
	| ExtensionAdditionList "," ExtensionAddition
*/
#[derive(Debug)]
pub struct ExtensionAdditionList(Vec<ExtensionAddition>);

impl ExtensionAdditionList {
	parser!(parse<Self> => map(
		delimited1(ExtensionAddition::parse, symbol(',')),
		|v| Self(v)
	));
}


/* ExtensionAddition ::= ComponentType | ExtensionAdditionGroup */
#[derive(Debug)]
pub enum ExtensionAddition {
	Type(ComponentType),
	Group(ExtensionAdditionGroup)
}

impl ExtensionAddition {
	parser!(parse<Self> => alt!(
		map(ComponentType::parse, |v| Self::Type(v)),
		map(ExtensionAdditionGroup::parse, |v| Self::Group(v))
	));
}


/* ExtensionAdditionGroup ::= "[[" VersionNumber ComponentTypeList "]]" */
#[derive(Debug)]
pub struct ExtensionAdditionGroup {
	pub version: Option<VersionNumber>,
	pub component_types: Vec<ComponentType>
}

impl ExtensionAdditionGroup {
	parser!(parse<Self> => seq!(c => {
		c.next(sequence("[["))?;
		let version = c.next(VersionNumber::parse)?;
		let component_types = c.next(ComponentTypeList::parse)?.0;
		c.next(sequence("]]"))?;
		Ok(Self { version, component_types })
	}));
}


/* VersionNumber ::= empty | number ":" */
#[derive(Debug)]
pub struct VersionNumber(usize);

impl VersionNumber {
	parser!(parse<Option<Self>> => opt(seq!(c => {
		let n = c.next(number)?;
		c.next(symbol(':'))?;
		Ok(Self(n))
	})));
}


/* ComponentTypeList ::= ComponentType | ComponentTypeList "," ComponentType */
#[derive(Debug)]
pub struct ComponentTypeList(Vec<ComponentType>);

impl ComponentTypeList {
	parser!(parse<Self> => map(
		delimited1(ComponentType::parse, symbol(',')), |v| Self(v)
	));
}


/*
ComponentType ::=
	NamedType
	| NamedType OPTIONAL
	| NamedType DEFAULT Value
	| COMPONENTS OF Type
*/
#[derive(Debug)]
pub enum ComponentType {
	Field(ComponentField),
	ComponentsOf(Type) // Sequence of at least one element
}

#[derive(Debug)]
pub struct ComponentField {
	pub name: AsciiString,
	pub typ: Type,
	pub mode: ComponentMode
}

#[derive(Debug)]
pub enum ComponentMode {
	Required,
	Optional,
	WithDefault(Value)
}


impl ComponentType {
	parser!(parse<Self> => alt!(
		seq!(c => {
			let typ = c.next(NamedType::parse)?;

			if c.next(opt(reserved("OPTIONAL")))?.is_some() {
				return Ok(ComponentType::Field(ComponentField {
					name: typ.name,
					typ: typ.typ,
					mode: ComponentMode::Optional
				}));
			}

			let default = c.next(opt(seq!(c => {
				c.next(reserved("DEFAULT"))?;
				c.next(Value::parse)
			})))?;

			if let Some(value) = default {
				return Ok(Self::Field(ComponentField {
					name: typ.name,
					typ: typ.typ,
					mode: ComponentMode::WithDefault(value)
				})); 
			}

			Ok(Self::Field(ComponentField {
				name: typ.name,
				typ: typ.typ,
				mode: ComponentMode::Required
			}))
		}),
		seq!(c => {
			c.next(reserved("COMPONENTS"))?;
			c.next(reserved("OF"))?;
			let typ = c.next(Type::parse)?;
			Ok(Self::ComponentsOf(typ))
		})
	));
}


/* SequenceValue ::= "{" ComponentValueList "}" | "{" "}" */
#[derive(Debug)]
pub struct SequenceValue(pub Vec<NamedValue>);

impl SequenceValue {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol('{'))?;
		let inner = c.next(opt(ComponentValueList::parse))?
			.unwrap_or(ComponentValueList(vec![]));
		c.next(symbol('}'))?;
		Ok(Self(inner.0))
	}));
}


/* ComponentValueList ::= NamedValue | ComponentValueList "," NamedValue */
#[derive(Debug)]
pub struct ComponentValueList(Vec<NamedValue>);

impl ComponentValueList {
	parser!(parse<Self> => {
		map(delimited1(NamedValue::parse, symbol(',')), |arr| Self(arr))
	});
}


/* SequenceOfType ::= SEQUENCE OF Type | SEQUENCE OF NamedType */
#[derive(Debug)]
pub struct SequenceOfType(CollectionType);

impl SequenceOfType {
	parser!(parse<(Option<Constraint>, Self)> => seq!(c => {
		c.next(reserved("SEQUENCE"))?;
		let (constr, coll) = c.next(CollectionType::parse)?;
		Ok((constr, Self(coll)))
	}));
}


/*
SequenceOfValue ::=
	"{" ValueList "}"
	| "{" NamedValueList "}"
	| "{" "}"
*/
#[derive(Debug)]
pub enum SequenceOfValue {
	Values(Vec<Value>),
	NamedValues(Vec<NamedValue>),
	Empty
}

impl SequenceOfValue {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol('{'))?;
		
		let val = c.next(opt(alt!(
			map(ValueList::parse, |v| Self::Values(v.0)),
			map(NamedValueList::parse, |v| Self::NamedValues(v.0))
		)))?.unwrap_or(Self::Empty);

		c.next(symbol('}'))?;

		Ok(val)
	}));
}


/* ValueList ::= Value | ValueList "," Value */
#[derive(Debug)]
pub struct ValueList(Vec<Value>);

impl ValueList {
	parser!(parse<Self> => {
		map(delimited1(Value::parse, symbol(',')), |arr| ValueList(arr))
	});
}


/* NamedValueList ::= NamedValue | NamedValueList "," NamedValue */
#[derive(Debug)]
pub struct NamedValueList(Vec<NamedValue>);

impl NamedValueList {
	parser!(parse<Self> => map(
		delimited1(NamedValue::parse, symbol(',')),
		|v| Self(v)));
}



/*
SetType ::=
	SET "{" "}"
	| SET "{" ExtensionAndException OptionalExtensionMarker "}"
	| SET "{" ComponentTypeLists "}"
*/
#[derive(Debug)]
pub struct SetType(ComponentBody);

impl SetType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("SET"))?;
		let body = c.next(ComponentBody::parse)?;
		Ok(Self(body))
	}));
}


// TODO: This is the same as the SequenceValue.
/* SetValue ::= "{" ComponentValueList "}" | "{" "}" */


/* SetOfType ::= SET OF Type | SET OF NamedType */
#[derive(Debug)]
pub struct SetOfType(CollectionType);

impl SetOfType {
	parser!(parse<(Option<Constraint>, Self)> => seq!(c => {
		c.next(reserved("SET"))?;
		let (constr, coll) = c.next(CollectionType::parse)?;
		Ok((constr, Self(coll)))
	}));
}

#[derive(Debug)]
pub enum CollectionType {
	Type(Box<Type>),
	Named(Box<NamedType>)
}

impl CollectionType {
	parser!(parse<(Option<Constraint>, Self)> => seq!(c => {
		let constraint = c.next(opt(alt!(
			Constraint::parse,
			map(SizeConstraint::parse, |c| {
				Constraint {
					spec: ElementSetSpecs {
						root: ElementSetSpec::Unions(vec![
							Intersections(vec![IntersectionElements {
								elements: Elements::Subtype(SubtypeElements::Size(Box::new(c.0))),
								exclusions: None
						}])]),
						additional: None
					},
					exception: None
				}
			})
		)))?;

		c.next(reserved("OF"))?;

		let coll = c.next(alt!(
			map(Type::parse, |v| Self::Type(Box::new(v))),
			map(NamedType::parse, |v| Self::Named(Box::new(v)))
		))?;

		Ok((constraint, coll))
	}));
}




/*
SetOfValue ::=
	"{" ValueList "}"
	| "{" NamedValueList "}"
	| "{" "}"
*/

/* ChoiceType ::= CHOICE "{" AlternativeTypeLists "}" */
#[derive(Debug)]
pub struct ChoiceType {
	pub types: AlternativeTypeLists
}

impl ChoiceType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("CHOICE"))?;
		c.next(symbol('{'))?;
		let types = c.next(AlternativeTypeLists::parse)?;
		c.next(symbol('}'))?;
		Ok(Self { types })
	}));
}


/*
AlternativeTypeLists ::=
	RootAlternativeTypeList
	| RootAlternativeTypeList ","
	  ExtensionAndException ExtensionAdditionAlternatives
	  OptionalExtensionMarker
*/
#[derive(Debug)]
pub struct AlternativeTypeLists {
	pub types: Vec<NamedType>
}

impl AlternativeTypeLists {
	parser!(parse<Self> => map(RootAlternativeTypeList::parse, |v| Self {
		types: v.0
	}));
}

/* RootAlternativeTypeList ::= AlternativeTypeList */
#[derive(Debug)]
pub struct RootAlternativeTypeList {}

impl RootAlternativeTypeList {
	parser!(parse<AlternativeTypeList> => AlternativeTypeList::parse);
}


/*
ExtensionAdditionAlternatives ::=
	"," ExtensionAdditionAlternativesList
	| empty
*/

/*
ExtensionAdditionAlternativesList ::=
	ExtensionAdditionAlternative
	| ExtensionAdditionAlternativesList "," ExtensionAdditionAlternative
*/

/*
ExtensionAdditionAlternative ::=
	ExtensionAdditionAlternativesGroup
	| NamedType
*/


/*
ExtensionAdditionAlternativesGroup ::=
	"[[" VersionNumber AlternativeTypeList "]]"
*/
#[derive(Debug)]
pub struct ExtensionAdditionAlternativesGroup {
	pub version: Option<VersionNumber>,
	pub types: Vec<NamedType>
}

impl ExtensionAdditionAlternativesGroup {
	parser!(parse<Self> => seq!(c => {
		c.next(sequence("[["))?;
		let version = c.next(VersionNumber::parse)?;
		let types = c.next(AlternativeTypeList::parse)?.0;
		c.next(sequence("]]"))?;
		Ok(Self { version, types })
	}));
}


/* AlternativeTypeList ::= NamedType | AlternativeTypeList "," NamedType */
#[derive(Debug)]
pub struct AlternativeTypeList(Vec<NamedType>);

impl AlternativeTypeList {
	parser!(parse<Self> => map(
		delimited1(NamedType::parse, symbol(',')),
		|v| Self(v)
	));
}


/* ChoiceValue ::= identifier ":" Value */
#[derive(Debug)]
pub struct ChoiceValue {
	pub key: AsciiString,
	pub value: Value
}

impl ChoiceValue {
	parser!(parse<Option<Self>> => opt(seq!(c => {
		let key = c.next(identifier)?;
		c.next(symbol(':'))?;
		let value = c.next(Value::parse)?;
		Ok(Self { key, value })
	})));
}


/* SelectionType ::= identifier "<" Type */

/* PrefixedType ::= TaggedType | EncodingPrefixedType */


#[derive(Debug)]
pub enum TypePrefix {
	Tag(TagPrefix),
	Encoding(EncodingPrefix)
}

impl TypePrefix {
	parser!(parse<Self> => alt!(
		map(TagPrefix::parse, |v| Self::Tag(v)),
		map(EncodingPrefix::parse, |v| Self::Encoding(v))
	));
}


/*
TaggedType ::=
	Tag Type
	| Tag IMPLICIT Type
	| Tag EXPLICIT Type
*/
#[derive(Debug)]
pub struct TagPrefix {
	pub tag: Tag,
	pub mode: Option<TagMode>
}

impl TagPrefix {
	parser!(parse<Self> => seq!(c => {
		let tag = c.next(Tag::parse)?;
		let mode = c.next(opt(alt!(
			map(reserved("IMPLICIT"), |_| TagMode::Explicit),
			map(reserved("EXPLICIT"), |_| TagMode::Implicit)
		)))?;

		Ok(Self { tag, mode })
	}));
}

#[derive(Debug, Clone, Copy)]
pub enum TagMode {
	Implicit,
	Explicit
}

/* Tag ::= "[" EncodingReference Class ClassNumber "]" */
#[derive(Debug)]
pub struct Tag {
	pub encoding_ref: Option<EncodingReference>,
	pub class: TagClass,
	pub number: ClassNumber
}

impl Tag {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol('['))?;
		let encoding_ref = c.next(EncodingReference::parse)?;
		let class = c.next(parse_class)?;
		let number = c.next(ClassNumber::parse)?;
		c.next(symbol(']'))?;
		Ok(Self { encoding_ref, class, number })
	}));
}

/* EncodingPrefixedType ::= EncodingPrefix Type */
/* EncodingPrefix ::= "[" EncodingReference EncodingInstruction "]" */
#[derive(Debug)]
pub struct EncodingPrefix {
	pub reference: Option<EncodingReference>,
	pub instruction: Bytes
}

impl EncodingPrefix {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol('['))?;
		let reference = c.next(EncodingReference::parse)?;
		let instruction = c.next(take_while(|c| c != ']' as u8))?;
		c.next(symbol(']'))?;
		Ok(Self { reference, instruction })
	}));
}

/* EncodingReference ::= encodingreference ":" | empty */
#[derive(Debug)]
pub struct EncodingReference(AsciiString);

impl EncodingReference {
	parser!(parse<Option<Self>> => opt(seq!(c => {
		let s = c.next(encodingreference)?;
		c.next(symbol(':'))?;
		Ok(Self(s))
	})));
}


/* ClassNumber ::= number | DefinedValue */
#[derive(Debug)]
pub enum ClassNumber {
	Immediate(usize),
	Defined(DefinedValue)
}

impl ClassNumber {
	parser!(parse<Self> => alt!(
		map(number, |v| Self::Immediate(v)),
		map(DefinedValue::parse, |v| Self::Defined(v))
	));
}


/* Class ::= UNIVERSAL | APPLICATION | PRIVATE | empty */
parser!(parse_class<TagClass> => map(opt(alt!(
	map(reserved("UNIVERSAL"), |_| TagClass::Universal),
	map(reserved("APPLICATION"), |_| TagClass::Application),
	map(reserved("PRIVATE"), |_| TagClass::Private)
)), |v| v.unwrap_or(TagClass::ContextSpecific)));


/* ObjectIdentifierType ::= OBJECT IDENTIFIER */
#[derive(Debug)]
pub struct ObjectIdentifierType {}

impl ObjectIdentifierType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("OBJECT"))?;
		c.next(reserved("IDENTIFIER"))?;
		Ok(Self {})
	}));
}


/* IRIType ::= OID-IRI */

/*
IRIValue ::=
	"""
	FirstArcIdentifier
	SubsequentArcIdentifier
	"""
*/

/* FirstArcIdentifier ::= 	"/" ArcIdentifier */

/*
SubsequentArcIdentifier ::=
	"/" ArcIdentifier SubsequentArcIdentifier
	| empty
*/

/*
ArcIdentifier ::=
	integerUnicodeLabel
	| non-integerUnicodeLabel
*/

/* RelativeIRIType ::= RELATIVE-OID-IRI */

/*
RelativeIRIValue ::=
	"""
	FirstRelativeArcIdentifier
	SubsequentArcIdentifier
	"""
*/

/*
FirstRelativeArcIdentifier ::=
	ArcIdentifier
*/

/* EmbeddedPDVType ::= EMBEDDED PDV */
#[derive(Debug)]
pub struct EmbeddedPDVType {}

impl EmbeddedPDVType {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("EMBEDDED"))?;
		c.next(reserved("PDV"))?;
		Ok(Self {})
	}));
}

/* EmbeddedPDVValue ::= SequenceValue */

/* ExternalType ::= EXTERNAL */

/* ExternalValue ::= SequenceValue */


#[derive(Debug)]
pub enum CharacterStringType {
	Restricted(RestrictedCharacterStringType), Unrestricted
}

impl CharacterStringType {
	parser!(parse<Self> => { alt!(
		map(RestrictedCharacterStringType::parse, |v| Self::Restricted(v)),
		map(UnrestrictedCharacterStringType::parse, |_| Self::Unrestricted)
	) });
}


/*
CharacterStringValue ::=
	RestrictedCharacterStringValue
	| UnrestrictedCharacterStringValue
*/


#[derive(Debug)]
pub enum RestrictedCharacterStringType {
	BMPString,
	GeneralString,
	GraphicString,
	IA5String,
	ISO646String,
	NumericString,
	PrintableString,
	TeletexString,
	T61String,
	UniversalString,
	UTF8String,
	VideotexString,
	VisibleString
}

impl RestrictedCharacterStringType {
	parser!(parse<Self> => { alt!(
		map(reserved("BMPString"), |_| Self::BMPString),
		map(reserved("GeneralString"), |_| Self::GeneralString),
		map(reserved("GraphicString"), |_| Self::GraphicString),
		map(reserved("IA5String"), |_| Self::IA5String),
		map(reserved("ISO646String"), |_| Self::ISO646String),
		map(reserved("NumericString"), |_| Self::NumericString),
		map(reserved("PrintableString"), |_| Self::PrintableString),
		map(reserved("TeletexString"), |_| Self::TeletexString),
		map(reserved("T61String"), |_| Self::T61String),
		map(reserved("UniversalString"), |_| Self::UniversalString),
		map(reserved("UTF8String"), |_| Self::UTF8String),
		map(reserved("VideotexString"), |_| Self::VideotexString),
		map(reserved("VisibleString"), |_| Self::VisibleString)
	) });

	pub fn typename(&self) -> &'static str {
		use RestrictedCharacterStringType::*;
		match self {
			BMPString => "BMPString",
			GeneralString => "GeneralString",
			GraphicString => "GraphicString",
			IA5String => "IA5String",
			ISO646String => "ISO646String",
			NumericString => "NumericString",
			PrintableString => "PrintableString",
			TeletexString => "TeletexString",
			T61String => "T61String",
			UniversalString => "UniversalString",
			UTF8String => "UTF8String",
			VideotexString => "VideotexString",
			VisibleString => "VisibleString"
		}
	}
}


/*
RestrictedCharacterStringValue ::=
	cstring
	| CharacterStringList
	| Quadruple
	| Tuple
*/

/* CharacterStringList ::= "{" CharSyms "}" */

/*
CharSyms ::=
	CharsDefn
	| CharSyms "," CharsDefn
*/

/*
CharsDefn ::=
	cstring
	| Quadruple
	| Tuple
	| DefinedValue
*/

/* Quadruple ::= "{" Group "," Plane "," Row "," Cell "}" */

/* Group ::= number */
#[derive(Debug)]
pub struct Group(usize);

impl Group {
	parser!(parse<Self> => map(number, |v| Self(v)));
}

/* Plane ::= number */
#[derive(Debug)]
pub struct Plane(usize);

impl Plane {
	parser!(parse<Self> => map(number, |v| Self(v)));
}

/* Row ::= number */
#[derive(Debug)]
pub struct Row(usize);

impl Row {
	parser!(parse<Self> => map(number, |v| Self(v)));
}

/* Cell ::= number */
#[derive(Debug)]
pub struct Cell(usize);

impl Cell {
	parser!(parse<Self> => map(number, |v| Self(v)));
}

/* Tuple ::= "{" TableColumn "," TableRow "}" */
#[derive(Debug)]
struct Tuple {
	column: usize,
	row: usize
}

impl Tuple {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol('{'))?;
		let column = c.next(TableColumn::parse)?.0;
		c.next(symbol(','))?;
		let row = c.next(TableRow::parse)?.0;
		c.next(symbol('}'))?;

		Ok(Self { column, row })
	}));
}


/* TableColumn ::= number */
#[derive(Debug)]
pub struct TableColumn(usize);

impl TableColumn {
	parser!(parse<Self> => map(number, |v| Self(v)));
}


/* TableRow ::= number */
#[derive(Debug)]
pub struct TableRow(usize);

impl TableRow {
	parser!(parse<Self> => map(number, |v| Self(v)));
}


/* UnrestrictedCharacterStringType ::= CHARACTER STRING */
#[derive(Debug)]
pub struct UnrestrictedCharacterStringType {}

impl UnrestrictedCharacterStringType {
	parser!(parse<Self> => {
		seq!(c => {
			c.next(reserved("CHARACTER"))?;
			c.next(reserved("STRING"))?;
			Ok(Self {})
		})
	});
}


/* UnrestrictedCharacterStringValue ::= SequenceValue */
/* UsefulType ::= typereference */
#[derive(Debug)]
pub struct UsefulType(AsciiString);

impl UsefulType {
	parser!(parse<Self> => map(typereference, |v| Self(v)));
}



/* Constraint ::= "(" ConstraintSpec ExceptionSpec ")" */
#[derive(Debug)]
pub struct Constraint {
	pub spec: ElementSetSpecs,
	pub exception: Option<ExceptionSpec>
}

impl Constraint {
	parser!(parse<Self> => seq!(c => {
		c.next(symbol('('))?;
		let spec = c.next(ConstraintSpec::parse)?.0;
		let exception = c.next(ExceptionSpec::parse)?;
		c.next(symbol(')'))?;
		Ok(Self { spec, exception })
	}));
}


/* ConstraintSpec ::= SubtypeConstraint | GeneralConstraint */
#[derive(Debug)]
pub struct ConstraintSpec(ElementSetSpecs);

impl ConstraintSpec {
	parser!(parse<Self> => map(SubtypeConstraint::parse,
							   |v| Self(v)));
}


/* SubtypeConstraint ::= ElementSetSpecs */
pub struct SubtypeConstraint {}

impl SubtypeConstraint {
	parser!(parse<ElementSetSpecs> => ElementSetSpecs::parse);
}


/*
ElementSetSpecs ::=
	RootElementSetSpec
	| RootElementSetSpec "," "..."
	| RootElementSetSpec "," "..." "," AdditionalElementSetSpec
*/
#[derive(Debug)]
pub struct ElementSetSpecs {
	pub root: ElementSetSpec,
	pub additional: Option<ElementSetSpec>
}

impl ElementSetSpecs {
	parser!(parse<Self> => seq!(c => {
		let root = c.next(RootElementSetSpec::parse)?;
		let additional = c.next(opt(seq!(c => {
			c.next(symbol(','))?;
			c.next(sequence("..."))?;

			let val = c.next(opt(seq!(c => {
				c.next(symbol(','))?;
				c.next(AdditionalElementSetSpec::parse)
			})))?;

			Ok(val)
		})))?.unwrap_or(None);

		Ok(Self { root, additional })
	}));
}


/* RootElementSetSpec ::= ElementSetSpec */
pub struct RootElementSetSpec {}

impl RootElementSetSpec {
	parser!(parse<ElementSetSpec> => ElementSetSpec::parse);
}


/* AdditionalElementSetSpec ::= ElementSetSpec */
pub struct AdditionalElementSetSpec {}

impl AdditionalElementSetSpec {
	parser!(parse<ElementSetSpec> => ElementSetSpec::parse);
}


/* ElementSetSpec ::= Unions | ALL Exclusions */
#[derive(Debug)]
pub enum ElementSetSpec {
	Unions(Vec<Intersections>),
	Exclusions(Exclusions)
}

impl ElementSetSpec {
	parser!(parse<Self> => alt!(
		map(Unions::parse, |v| Self::Unions(v.0)),
		seq!(c => {
			c.next(reserved("ALL"))?;
			let val = c.next(Exclusions::parse)?;
			Ok(Self::Exclusions(val))
		})
	));
}


#[derive(Debug)]
pub struct Unions(Vec<Intersections>);

impl Unions {
	parser!(parse<Self> => map(
		delimited1(Intersections::parse, UnionMark::parse),
		|v| Self(v)));
}


#[derive(Debug)]
pub struct Intersections(Vec<IntersectionElements>);

impl Intersections {
	parser!(parse<Self> => map(
		delimited1(IntersectionElements::parse, IntersectionMark::parse),
		|v| Intersections(v)));
}


/* IntersectionElements ::= Elements | Elems Exclusions */
/* Elems ::= Elements */
#[derive(Debug)]
pub struct IntersectionElements {
	pub elements: Elements,
	pub exclusions: Option<Exclusions>
}

impl IntersectionElements {
	parser!(parse<Self> => seq!(c => {
		let elements = c.next(Elements::parse)?;
		let exclusions = c.next(opt(Exclusions::parse))?;
		Ok(Self { elements, exclusions })
	}));
}


/* Exclusions ::= EXCEPT Elements */
#[derive(Debug)]
pub struct Exclusions(Elements);

impl Exclusions {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("EXCEPT"))?;
		let elements = c.next(Elements::parse)?;
		Ok(Self(elements))
	}));
}


/* UnionMark ::= "|" | UNION */
struct UnionMark {}

impl UnionMark {
	parser!(parse<()> => alt!(symbol('|'), reserved("UNION")));
}


/* IntersectionMark ::= "^" | INTERSECTION */
struct IntersectionMark {}

impl IntersectionMark {
	parser!(parse<()> => alt!(symbol('^'), reserved("INTERSECTION")));
}


/*
Elements ::=
	SubtypeElements
	| ObjectSetElements
	| "(" ElementSetSpec ")"
*/
#[derive(Debug)]
pub enum Elements {
	Subtype(SubtypeElements),
	Wrapped(Box<ElementSetSpec>)
}

impl Elements {
	parser!(parse<Self> => alt!(
		map(SubtypeElements::parse, |v| Self::Subtype(v)),
		seq!(c => {
			c.next(symbol('('))?;
			let inner = c.next(ElementSetSpec::parse)?;
			c.next(symbol(')'))?;
			Ok(Self::Wrapped(Box::new(inner)))
		})
	));
}


/*
SubtypeElements ::=
	SingleValue
	| ContainedSubtype
	| ValueRange
	| PermittedAlphabet
	| SizeConstraint
	| TypeConstraint
	| InnerTypeConstraints
	| PatternConstraint
	| PropertySettings
	| DurationRange
	| TimePointRange
	| RecurrenceRange
*/
#[derive(Debug)]
pub enum SubtypeElements {
	SingleValue(Value), /* SingleValue ::= Value */
	ContainedSubtype,
	ValueRange(ValueRange),
	PermittedAlphabet,
	Size(Box<Constraint>),
	Type(Type), /* TypeConstraint ::= Type */
	InnerTypeConstraints,
	PatternConstraint(Value),
	PropertySettings
}

impl SubtypeElements {
	parser!(parse<Self> => alt!(
		// TODO: Incomplete
		map(Type::parse, |v| Self::Type(v)),
		map(SizeConstraint::parse, |v| Self::Size(Box::new(v.0))),
		map(ValueRange::parse, |v| Self::ValueRange(v)),
		map(Value::parse, |v| Self::SingleValue(v))
	));
}


/* ContainedSubtype ::= Includes Type */
#[derive(Debug)]
pub struct ContainedSubtype {
	pub typ: Type
}

impl ContainedSubtype {
	parser!(parse<Self> => seq!(c => {
		c.next(Includes::parse)?;
		let typ = c.next(Type::parse)?;
		Ok(Self { typ })
	}));
}


/* Includes ::= INCLUDES | empty */
struct Includes {}

impl Includes {
	parser!(parse<Option<()>> => opt(reserved("INCLUDES")));
}


/* ValueRange ::= LowerEndpoint ".." UpperEndpoint */
#[derive(Debug)]
pub struct ValueRange {
	pub lower: LowerEndpoint,
	pub upper: UpperEndpoint
}

impl ValueRange {
	parser!(parse<Self> => seq!(c => {
		let lower = c.next(LowerEndpoint::parse)?;
		c.next(sequence(".."))?;
		let upper = c.next(UpperEndpoint::parse)?;
		Ok(Self { lower, upper })
	}));
}


/* LowerEndpoint ::= LowerEndValue | LowerEndValue "<" */
#[derive(Debug)]
pub struct LowerEndpoint {
	pub value: LowerEndValue,
	pub inclusive: bool 
}

impl LowerEndpoint {
	parser!(parse<Self> => seq!(c => {
		let value = c.next(LowerEndValue::parse)?;
		let inclusive = c.next(opt(symbol('<')))?.is_none();
		Ok(Self { value, inclusive })
	}));
}


/* UpperEndpoint ::= UpperEndValue | "<" UpperEndValue */
#[derive(Debug)]
pub struct UpperEndpoint {
	pub value: UpperEndValue,
	pub inclusive: bool 
}

impl UpperEndpoint {
	parser!(parse<Self> => seq!(c => {
		let value = c.next(UpperEndValue::parse)?;
		let inclusive = c.next(opt(symbol('<')))?.is_none();
		Ok(Self { value, inclusive })
	}));
}


/* LowerEndValue ::= Value | MIN */
#[derive(Debug)]
pub enum LowerEndValue {
	Value(Value),
	Min
}

impl LowerEndValue {
	parser!(parse<Self> => alt!(
		map(Value::parse, |v| Self::Value(v)),
		map(reserved("MIN"), |_| Self::Min)
	));
}


/* UpperEndValue ::= Value | MAX */
#[derive(Debug)]
pub enum UpperEndValue {
	Value(Value),
	Max
}

impl UpperEndValue {
	parser!(parse<Self> => alt!(
		map(Value::parse, |v| Self::Value(v)),
		map(reserved("MAX"), |_| Self::Max)
	));
}


/* SizeConstraint ::= SIZE Constraint */
pub struct SizeConstraint(Constraint);

impl SizeConstraint {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("SIZE"))?;
		let c = c.next(Constraint::parse)?;
		Ok(Self(c))
	}));
}



/* PermittedAlphabet ::= FROM Constraint */

/*
InnerTypeConstraints ::=
	WITH COMPONENT SingleTypeConstraint
	| WITH COMPONENTS MultipleTypeConstraints
*/

/* SingleTypeConstraint::= Constraint */

/* MultipleTypeConstraints ::= FullSpecification | PartialSpecification */

/* FullSpecification ::= "{" TypeConstraints "}" */

/* PartialSpecification ::= "{" "..." "," TypeConstraints "}" */

/*
TypeConstraints ::=
	NamedConstraint
	| NamedConstraint "," TypeConstraints
*/

/* NamedConstraint ::= identifier ComponentConstraint */

/* ComponentConstraint ::= ValueConstraint PresenceConstraint */

/* ValueConstraint ::= Constraint | empty */

/* PresenceConstraint ::= PRESENT | ABSENT | OPTIONAL | empty */
#[derive(Debug)]
pub enum PresenceConstraint {
	Present,
	Absent,
	Optional
}

impl PresenceConstraint {
	parser!(parse<Option<Self>> => {
		opt(alt!(
			map(reserved("PRESENT"), |_| PresenceConstraint::Present),
			map(reserved("ABSENT"), |_| PresenceConstraint::Absent),
			map(reserved("OPTIONAL"), |_| PresenceConstraint::Optional)
		))
	});
}

/* PatternConstraint ::= PATTERN Value */
#[derive(Debug)]
pub struct PatternConstraint(Value);

impl PatternConstraint {
	parser!(parse<Self> => seq!(c => {
		c.next(reserved("PATTERN"))?;
		c.next(map(Value::parse, |v| Self(v)))
	}));
}

/* PropertySettings ::= SETTINGS simplestring */

/* PropertyName ::= psname */
pub type PropertyName = AsciiString;
parser!(parse_property_name<AsciiString> => psname);

/* SettingName ::= psname */
pub type SettingName = AsciiString;
parser!(parse_setting_name<AsciiString> => psname);

pub type DurationRange = ValueRange;

pub type TimePointRange = ValueRange;

pub type RecurrenceRange = ValueRange;

/* ExceptionSpec ::= "!" ExceptionIdentification | empty */
#[derive(Debug)]
pub struct ExceptionSpec(ExceptionIdentification);

impl ExceptionSpec {
	parser!(parse<Option<Self>> => opt(map(ExceptionIdentification::parse,
										   |v| Self(v))));
}

/*
ExceptionIdentification ::=
	SignedNumber
	| DefinedValue
	| Type ":" Value
*/
#[derive(Debug)]
pub enum ExceptionIdentification {
	Immediate(isize),
	Defined(DefinedValue),
	TypeValue(Box<Type>, Value)
}

impl ExceptionIdentification {
	parser!(parse<Self> => alt!(
		map(SignedNumber::parse, |v| Self::Immediate(v.0)),
		map(DefinedValue::parse, |v| Self::Defined(v)),
		seq!(c => {
			let typ = c.next(Type::parse)?;
			c.next(symbol(':'))?;
			let val = c.next(Value::parse)?;
			Ok(Self::TypeValue(Box::new(typ), val))
		})
	));
}

/*
TODO: X681
OPERATION ::= CLASS
{
	&ArgumentType
	&ResultType
	&Errors
	&Linked
	&resultReturned
	&operationCode
}
WITH SYNTAX
{
	[ARGUMENT
	[RESULT
	[RETURN RESULT
	[ERRORS
	[LINKED
	CODE
}


- typefieldreference and the associated ones are the lexical items that start
					 with '&'

*/


#[cfg(test)]
mod tests {
	use super::*;

	use std::io::Read;

	#[test]
	fn asn1_syntax_test() {
		
		// let (v, _) = complete(Constraint::parse)(Bytes::from("(SET(1..3))")).unwrap();
		// println!("{:?}", v);

		// return;

		// PKIX1Explicit88
		let mut file = std::fs::File::open("/home/dennis/workspace/dacha/pkg/crypto/src/asn/PKIX1Explicit88.asn1").unwrap();
		let mut data = vec![];
		file.read_to_end(&mut data).unwrap();

		let (module, _) = complete(ModuleDefinition::parse)(Bytes::from(data))
			.unwrap();

		println!("{:#?}", module);
	}
}
