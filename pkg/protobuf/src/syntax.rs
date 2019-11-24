// Syntax of the .proto files for version 2
// Based on https://developers.google.com/protocol-buffers/docs/reference/proto2-spec
//
// https://developers.google.com/protocol-buffers/docs/reference/proto3-spec

// |   alternation
// ()  grouping
// []  option (zero or one time)
// {}  repetition (any number of times)


use super::tokenizer::{Token, Tokenizer, capitalLetter, decimalDigit, letter};
use super::spec::*;

#[derive(Debug)]
pub enum ParseError {
	// When there are not enough symbols to continue parsing.
	Incomplete,
	// When the next set of symbols does not 
	Failure
}

struct ParserState<'a> {
	input: &'a [Token],
	state: Syntax
}

// The value will be the remaining input after the parsed portion. 
type ParseResult<'a, T> = std::result::Result<(T, &'a [Token]), ParseError>;

type ParseOption<'a, T> = Option<(T, &'a [Token])>;

trait Parser<'a, T> = Fn(&'a [Token]) -> ParseResult<'a, T>;
trait ParserMut<'a, T> = FnMut(&'a [Token]) -> ParseResult<'a, T>;


/// A parser that accepts and outputs a single atomic item.
trait AtomParser<'a> = Fn(&'a [Token]) -> ParseResult<&'a Token>;

#[derive(Clone)]
struct ParseCursor<'a> {
	rest: &'a [Token]
}

impl<'a> ParseCursor<'a> {
	fn new(input: &[Token]) -> ParseCursor {
		ParseCursor { rest: input }
	}
	
	fn unwrap_with<T>(self, value: T) -> ParseResult<'a, T> {
		Ok((value, self.rest))
	}

	// Runs a parser on the remaining input, advancing the cursor if successful.
	// On parser error, the cursor will stay the same.
	fn next<T, F: Parser<'a, T>>(&mut self, f: F) -> std::result::Result<T, ParseError> {
		match f(self.rest) {
			Ok((v, r)) => {
				self.rest = r;
				Ok(v)
			},
			Err(e) => Err(e)
		}
	}

	fn is<T: PartialEq<Y>, Y, F: Parser<'a, T>>(&mut self, f: F, v: Y) -> std::result::Result<T, ParseError> {
		match f(self.rest) {
			Ok((v2, r)) => {
				if v2 != v {
					return Err(ParseError::Failure);
				}

				self.rest = r;
				Ok(v2)
			},
			Err(e) => Err(e)
		}
	}

	// Runs a parser as many times as possible returning a vector of all results
	fn many<T, F: Parser<'a, T>>(&mut self, f: F) -> Vec<T> {
		let mut results = vec![];
		while let Ok((v, r)) = f(self.rest) {
			self.rest = r;
			results.push(v);
		}

		results
	}

	// Accepts any number of items parsed by f separated by a delimiter
	// parsed by d.
	fn delimited<T: Clone, Y, F: Clone + Parser<'a, T>,
				 D: Clone + Parser<'a, Y>>(&mut self, f: F, d: D) -> Vec<T> {
		let mut vals = vec![];
		let first = match self.next(f.clone()) {
			Ok(v) => v,
			Err(e) => { return vals; }
		};
		vals.push(first);
		vals.extend_from_slice(&self.many(|input| -> ParseResult<T> {
			let mut c = ParseCursor::new(input);
			c.next(d.clone())?;
			let v = c.next(f.clone())?;
			c.unwrap_with(v)
		}));

		vals
	}

	
}

fn atom(input: &[Token]) -> ParseResult<&Token> {
	if input.len() < 1 {
		Err(ParseError::Incomplete)
	} else {
		Ok((&input[0], &input[1..]))
	}
}

// TODO: Not really used anywhere
fn opt<T>(res: ParseResult<T>) -> ParseOption<T> {
	match res {
		Ok(x) => Some(x),
		Err(_) => None
	}
}

macro_rules! token_atom {
	($name:ident, $e:ident, $t:ty) => {
		fn $name(input: &[Token]) -> ParseResult<$t> {
			match atom(input)? {
				(Token::$e(s), rest) => Ok((s.clone(), rest)),
				_ => Err(ParseError::Failure)
			}
		}
	};
}

// Wrappers for reading a single type of token and returning the inner representation
token_atom!(ident, Identifier, String);
token_atom!(floatLit, Float, f64);
token_atom!(intLit, Integer, usize);
token_atom!(symbol, Symbol, char);
token_atom!(strLit, String, String);


macro_rules! alt {
	( $input:expr, $first:expr, $( $next:expr ),* ) => {
		($first)($input)
			$(
				.or_else(|_| ($next)($input))
			)*
	};
}


fn map<'a, T, Y, P: Parser<'a, T>, F: Fn(T) -> Y>(p: P, f: F) -> impl Parser<'a, Y> {
	move |input: &'a [Token]| {
		p(input).map(|(v, rest)| (f(v), rest))
	}
}

// Proto 2 and 3
// fullIdent = ident { "." ident }
fn fullIdent(input: &[Token]) -> ParseResult<String> {
	let mut c = ParseCursor::new(input);
	
	let mut id = c.next(ident)?;

	while let Ok('.') = c.next(symbol) {
		id.push('.');

		let id_more = c.next(ident)?;
		id.push_str(id_more.as_str());
	}
	

	c.unwrap_with(id)
}



// Proto 2 and 3
fn enumName(input: &[Token]) -> ParseResult<String> { ident(input) }
fn messageName(input: &[Token]) -> ParseResult<String> { ident(input) }
fn fieldName(input: &[Token]) -> ParseResult<String> { ident(input) }
fn oneofName(input: &[Token]) -> ParseResult<String> { ident(input) }
fn mapName(input: &[Token]) -> ParseResult<String> { ident(input) }
fn serviceName(input: &[Token]) -> ParseResult<String> { ident(input) }
fn rpcName(input: &[Token]) -> ParseResult<String> { ident(input) }
fn streamName(input: &[Token]) -> ParseResult<String> { ident(input) }

// Proto 2 and 3
// messageType = [ "." ] { ident "." } messageName
fn messageType(input: &[Token]) -> ParseResult<String> {
	let mut c = ParseCursor::new(input);

	let mut s = String::new();
	if let Ok(dot) = c.is(symbol, '.') {
		s.push(dot);
	}

	let path = c.many(|input| {
		let mut c = ParseCursor::new(input);
		let mut id = c.next(ident)?;
		id.push(c.is(symbol, '.')?);
		c.unwrap_with(id)
	});

	s.push_str(&path.join(""));

	let name = c.next(messageName)?;
	s.push_str(name.as_str());

	c.unwrap_with(s)
}

// Proto 2 and 3
// enumType = [ "." ] { ident "." } enumName
fn enumType(input: &[Token]) -> ParseResult<String> {
	// TODO: Instead internally use enumName instead of messageName
	messageType(input)
}

// Proto 2
// groupName = capitalLetter { letter | decimalDigit | "_" }
fn groupName(input: &[Token]) -> ParseResult<String> {
	let (id, rest) = ident(input)?;

	for (i, c) in id.chars().enumerate() {
		let valid = if i == 0 {
			capitalLetter(c)
		} else {
			letter(c) || decimalDigit(c) || c == '_'
		};

		if !valid {
			return Err(ParseError::Failure);
		}
	}

	Ok((id, rest))
}

// Proto 2 and 3
// boolLit = "true" | "false" 
fn boolLit(input: &[Token]) -> ParseResult<bool> {
	let (id, rest) = ident(input)?;
	let val = match id.as_ref() {
		"true" => true,
		"false" => false,
		_ => return Err(ParseError::Failure)
	};
	
	Ok((val, rest))
}


// Proto 2 and 3
// emptyStatement = ";"
fn emptyStatement(input: &[Token]) -> ParseResult<()> {
	symbol(input).and_then(|(c, rest)| {
		if c == ';' {
			Ok(((), rest))
		} else {
			Err(ParseError::Failure)
		}
	})
}

// Proto 2 and 3
// constant = fullIdent | ( [ "-" | "+" ] intLit ) | ( [ "-" | "+" ] floatLit ) |
//                 strLit | boolLit 
fn constant(input: &[Token]) -> ParseResult<Constant> {
	let sign = |input| -> ParseResult<isize> {
		let (c, rest) = symbol(input)?;
		match c {
			'+' => Ok((1, rest)),
			'-' => Ok((-1, rest)),
			_ => Err(ParseError::Failure)
		}
	};

	// TODO: Can be combined with float_const
	let int_const = |input| {
		let mut c = ParseCursor::new(input);
		let sign: isize = c.next(sign).unwrap_or(1);
		let f = c.next(intLit)?;
		c.unwrap_with(Constant::Integer(sign * (f as isize)))
	};

	let float_const = |input| {
		let mut c = ParseCursor::new(input);
		let sign: isize = c.next(sign).unwrap_or(1);
		let f = c.next(floatLit)?;
		c.unwrap_with(Constant::Float((sign as f64) * f))
	};

	let str_const = |input| {
		strLit(input).map(|(s, rest)| (Constant::String(s), rest))
	};

	let bool_const = |input| {
		boolLit(input).map(|(b, rest)| (Constant::Bool(b), rest))
	};

	alt!(input,
		map(fullIdent, |s| Constant::Identifier(s)),
		int_const,
		float_const,
		str_const,
		bool_const
	)
}

// syntax = "syntax" "=" quote "proto2" quote ";"
pub fn syntax(input: &[Token]) -> ParseResult<Syntax> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "syntax")?;
	c.is(symbol, '=')?;
	let s = c.is(strLit, "proto2").map(|_| Syntax::Proto2)
		.or_else(|_| c.is(strLit, "proto3").map(|_| Syntax::Proto3))?;
	c.is(symbol, ';')?;
	c.unwrap_with(s)
}


// Proto 2 and 3
// import = "import" [ "weak" | "public" ] strLit ";" 
fn import(input: &[Token]) -> ParseResult<Import> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "import")?;

	let mut typ = c.is(ident, "weak").map(|_| ImportType::Weak)
		.or_else(|_| c.is(ident, "public").map(|_| ImportType::Public))
		.unwrap_or(ImportType::Default);
	let path = c.next(strLit)?;
	c.is(symbol, ';')?;
	c.unwrap_with(Import { typ, path })
}

// Proto 2 and 3
// package = "package" fullIdent ";"
fn package(input: &[Token]) -> ParseResult<String> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "package")?;
	let name = c.next(fullIdent)?;
	c.is(symbol, ';')?;
	c.unwrap_with(name)
}

// Proto 2 and 3
// option = "option" optionName  "=" constant ";"
fn option(input: &[Token]) -> ParseResult<Opt> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "option")?;
	let name = c.next(optionName)?;
	let value = c.next(constant)?;
	c.is(symbol, ';')?;
	c.unwrap_with(Opt { name, value })
}

// Proto 2 and 3
// optionName = ( ident | "(" fullIdent ")" ) { "." ident }
fn optionName(input: &[Token]) -> ParseResult<String> {
	let mut c = ParseCursor::new(input);
	let prefix = c.next(ident)
		.or_else(|_| c.next(|input| -> ParseResult<String> {
			let mut c = ParseCursor::new(input);
			c.is(symbol, '(')?;
			let s = c.next(fullIdent)?;
			c.is(symbol, ')');
			c.unwrap_with(String::from("(") + &s + &")")
		}))?;
	
	let rest = c.many(|input| {
		let mut c = ParseCursor::new(input);
		c.is(symbol, '.')?;
		let id = c.next(ident)?;
		c.unwrap_with(String::from(".") + &id)
	});

	c.unwrap_with(prefix + &rest.join(""))
}

// Proto 2
// label = "required" | "optional" | "repeated"
fn label(input: &[Token]) -> ParseResult<Label> {
	let mut c = ParseCursor::new(input);
	let label = c.is(ident, "required").map(|_| Label::Required)
		.or_else(|_| c.is(ident, "optional").map(|_| Label::Optional))
		.or_else(|_| c.is(ident, "repeated").map(|_| Label::Repeated))?;
	c.unwrap_with(label)
}

// Proto 2 and 3
// type = "double" | "float" | "int32" | "int64" | "uint32" | "uint64"
//       | "sint32" | "sint64" | "fixed32" | "fixed64" | "sfixed32" | "sfixed64"
//       | "bool" | "string" | "bytes" | messageType | enumType
fn fieldType(input: &[Token]) -> ParseResult<FieldType> {
	let mut c = ParseCursor::new(input);
	let primitive = |input| {
		let mut c = ParseCursor::new(input);
		let name = c.next(ident)?;
		let t = match name.as_str() {
			"double" => FieldType::Double,
			"float" => FieldType::Float,
			"int32" => FieldType::Int32,
			"int64" => FieldType::Int64,
			"uint32" => FieldType::Uint32,
			"uint64" => FieldType::Uint64,
			"sint32" => FieldType::Sint32,
			"sint64" => FieldType::Sint64,
			"fixed32" => FieldType::Fixed32,
			"fixed64" => FieldType::Sfixed64,
			"sfixed32" => FieldType::Sfixed32,
			"sfixed64" => FieldType::Sfixed64,
			"bool" => FieldType::Bool,
			"string" => FieldType::String,
			"bytes" => FieldType::Bytes,
			_ => { return Err(ParseError::Failure); }
		};

		c.unwrap_with(t)
	};

	let t = c.next(primitive)
		.or_else(|_| c.next(messageType).map(|n| FieldType::Named(n)))?;
	
	c.unwrap_with(t)
}

// Proto 2 and 3
// fieldNumber = intLit;
fn fieldNumber(input: &[Token]) -> ParseResult<usize> {
	intLit(input)
}

// TODO: In proto 3, 'label' should be replaced with '[ "repeated" ]'
// field = label type fieldName "=" fieldNumber [ "[" fieldOptions "]" ] ";"
fn field(input: &[Token]) -> ParseResult<Field> {
	let mut c = ParseCursor::new(input);
	let labl = c.next(label)?;	
	let typ = c.next(fieldType)?;
	let name = c.next(fieldName)?;
	c.is(symbol, '=')?;
	let num = c.next(fieldNumber)?;
	let unknown_options = c.next(fieldOptionsWrap).unwrap_or(vec![]);

	c.is(symbol, ';')?;

	c.unwrap_with(Field { label: labl, typ, name, num, options: FieldOptions::default(), unknown_options })
}

// Proto 2 and 3
// Not on the official grammar page, but useful to reuse.
// "[" fieldOptions "]"
fn fieldOptionsWrap(input: &[Token]) -> ParseResult<Vec<Opt>> {
	let mut c = ParseCursor::new(input);
	c.is(symbol, '[')?;
	let list = c.next(fieldOptions)?;
	c.is(symbol, ']')?;
	c.unwrap_with(list)
}

fn comma(input: &[Token]) -> ParseResult<char> {
	match symbol(input) {
		Ok((',', rest)) => Ok((',', rest)),
		Err(e) => Err(e),
		_ => Err(ParseError::Failure) 
	}
}

// Proto 2 and 3
// fieldOptions = fieldOption { ","  fieldOption }
fn fieldOptions(input: &[Token]) -> ParseResult<Vec<Opt>> {
	let mut c = ParseCursor::new(input);
	let opts = c.delimited(fieldOption, comma);
	if opts.len() < 1 {
		return Err(ParseError::Failure);
	}

	c.unwrap_with(opts)
}

// Proto 2 and 3
// fieldOption = optionName "=" constant
fn fieldOption(input: &[Token]) -> ParseResult<Opt> {
	let mut c = ParseCursor::new(input);
	let name = c.next(optionName)?;
	c.is(symbol, '=')?;
	let value = c.next(constant)?;
	c.unwrap_with(Opt { name, value })
}

// Proto 2
// group = label "group" groupName "=" fieldNumber messageBody
fn group(input: &[Token]) -> ParseResult<Group> {
	let mut c = ParseCursor::new(input);
	let lbl = c.next(label)?;
	c.is(ident, "group")?;
	let name = c.next(groupName)?;
	c.is(symbol, '=')?;
	let num = c.next(fieldNumber)?;
	let body = c.next(messageBody)?;
	c.unwrap_with(Group { label: lbl, name, num, body })
}

// Proto 2 and 3
// oneof = "oneof" oneofName "{" { oneofField | emptyStatement } "}"
fn oneof(input: &[Token]) -> ParseResult<OneOf> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "oneof")?;
	let name = c.next(oneofName)?;
	c.is(symbol, '{')?;
	let fields = c.many(|input| {
		let mut c = ParseCursor::new(input);
		let f = c.next(oneofField).map(|f| Some(f))
			.or_else(|_| c.next(emptyStatement).map(|_| None))?;
		c.unwrap_with(f)
	}).into_iter().filter_map(|x| x).collect::<Vec<_>>();
	c.is(symbol, '}')?;
	c.unwrap_with(OneOf { name, fields })
}

// Proto 2 and 3
// oneofField = type fieldName "=" fieldNumber [ "[" fieldOptions "]" ] ";"
fn oneofField(input: &[Token]) -> ParseResult<Field> {
	let mut c = ParseCursor::new(input);
	let typ = c.next(fieldType)?;
	let name = c.next(fieldName)?;
	c.is(symbol, '=')?;
	let num = c.next(fieldNumber)?;
	let unknown_options = c.next(fieldOptionsWrap).unwrap_or(vec![]);
	c.is(symbol, ';')?;
	c.unwrap_with(Field { label: Label::Optional, typ, name,
		num, options: FieldOptions::default(), unknown_options })
}

// Proto 2 and 3
// mapField = "map" "<" keyType "," type ">" mapName "=" fieldNumber [ "[" fieldOptions "]" ] ";"
fn mapField(input: &[Token]) -> ParseResult<MapField> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "map")?;
	c.is(symbol, '<')?;
	let key_type = c.next(keyType)?;
	c.is(symbol, ',')?;
	let value_type = c.next(fieldType)?;
	c.is(symbol, '>')?;
	let name = c.next(mapName)?;
	c.is(symbol, '=')?;
	let num = c.next(fieldNumber)?;
	let options = c.next(fieldOptionsWrap).unwrap_or(vec![]);
	c.is(symbol, ';')?;
	c.unwrap_with(MapField { key_type, value_type, name, num, options })
}

// Proto 2 and 3
// keyType = "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" |
//           "fixed32" | "fixed64" | "sfixed32" | "sfixed64" | "bool" | "string"
fn keyType(input: &[Token]) -> ParseResult<FieldType> {
	let mut c = ParseCursor::new(input);
	let name = c.next(ident)?;
	let t = match name.as_str() {
		"int32" => FieldType::Int32,
		"int64" => FieldType::Int64,
		"uint32" => FieldType::Uint32,
		"uint64" => FieldType::Uint64,
		"sint32" => FieldType::Sint32,
		"sint64" => FieldType::Sint64,
		"fixed32" => FieldType::Fixed32,
		"fixed64" => FieldType::Fixed64,
		"sfixed32" => FieldType::Sfixed32,
		"sfixed64" => FieldType::Sfixed64,
		"bool" => FieldType::Bool,
		"string" => FieldType::String,
		_ => { return Err(ParseError::Failure); }
	};

	c.unwrap_with(t)
}

// Proto 2
// extensions = "extensions" ranges ";"
fn extensions(input: &[Token]) -> ParseResult<Ranges> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "extensions")?;
	let out = c.next(ranges)?;
	c.is(symbol, ';')?;
	c.unwrap_with(out)
}

// Proto 2 and 3
// ranges = range { "," range }
fn ranges(input: &[Token]) -> ParseResult<Ranges> {
	let mut c = ParseCursor::new(input);
	let out = c.delimited(range, comma);
	if out.len() < 1 {
		return Err(ParseError::Failure);
	}

	c.unwrap_with(out)
}

// Proto 2 and 3
// range =  intLit [ "to" ( intLit | "max" ) ]
fn range(input: &[Token]) -> ParseResult<Range> {
	let mut c = ParseCursor::new(input);
	let lower = c.next(intLit)?;
	
	let upper_parser = |input| {
		let mut c = ParseCursor::new(input);
		c.is(ident, "to")?;
		let v = c.next(intLit)
			.or_else(|_| c.is(ident, "max").map(|_| std::usize::MAX))?;
		c.unwrap_with(v)
	};

	let upper = c.next(upper_parser)?;
	c.unwrap_with((lower, upper))
}

// Proto 2 and 3
// reserved = "reserved" ( ranges | fieldNames ) ";"
fn reserved(input: &[Token]) -> ParseResult<Reserved> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "reserved")?;
	let val = c.next(ranges).map(|rs| Reserved::Ranges(rs))
		.or_else(|_| c.next(fieldNames).map(|ns| Reserved::Fields(ns)))?;
	c.is(symbol, ';')?;
	c.unwrap_with(val)
}

// Proto 2 and 3
// fieldNames = fieldName { "," fieldName }
fn fieldNames(input: &[Token]) -> ParseResult<Vec<String>> {
	let mut c = ParseCursor::new(input);
	let mut out = c.delimited(fieldName, comma);
	if out.len() < 1 {
		return Err(ParseError::Failure);
	}

	c.unwrap_with(out)
}

// Proto 2 and 3
// enum = "enum" enumName enumBody
fn enum_(input: &[Token]) -> ParseResult<Enum> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "enum")?;
	let name = c.next(enumName)?;
	let body = c.next(enumBody)?;
	c.unwrap_with(Enum { name, body })
}

// Proto 2 and 3
// enumBody = "{" { option | enumField | emptyStatement } "}"
fn enumBody(input: &[Token]) -> ParseResult<Vec<EnumBodyItem>> {
	let mut c = ParseCursor::new(input);
	c.is(symbol, '{')?;
	let inner = c.many(|input| {
		let mut c = ParseCursor::new(input);
		let item = c.next(option).map(|o| Some(EnumBodyItem::Option(o)))
			.or_else(|_| c.next(enumField).map(|f| Some(EnumBodyItem::Field(f))))
			.or_else(|_| c.next(emptyStatement).map(|_| None))?;
		c.unwrap_with(item)
	}).into_iter().filter_map(|x| x).collect::<Vec<_>>();
	c.is(symbol, '}')?;
	c.unwrap_with(inner)
}

// Proto 2 and 3
// enumField = ident "=" intLit [ "[" enumValueOption { ","  enumValueOption } "]" ]";"
fn enumField(input: &[Token]) -> ParseResult<EnumField> {
	let mut c = ParseCursor::new(input);
	let name = c.next(ident)?;
	c.is(symbol, '=')?;
	let num = c.next(intLit)?;
	let options = c.next(|input| {
		let mut c = ParseCursor::new(input);
		c.is(symbol, '[')?;
		let opts = c.delimited(enumValueOption, comma);
		if opts.len() < 1 {
			return Err(ParseError::Failure);
		}
		c.is(symbol, ']')?;
		c.unwrap_with(opts)
	}).unwrap_or(vec![]);
	c.is(symbol, ';')?;

	c.unwrap_with(EnumField { name, num, options })
}

// Proto 2 and 3
// enumValueOption = optionName "=" constant
fn enumValueOption(input: &[Token]) -> ParseResult<Opt> {
	fieldOption(input)
}

// Proto 2 and 3
// message = "message" messageName messageBody
fn message(input: &[Token]) -> ParseResult<Message> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "message")?;
	let name = c.next(messageName)?;
	let body = c.next(messageBody)?;
	c.unwrap_with(Message { name, body })
}

// TODO: Proto3 has no 'extensions' or 'group'
// messageBody = "{" { field | enum | message | extend | extensions | group | option | oneof | mapField | reserved | emptyStatement } "}"
fn messageBody(input: &[Token]) -> ParseResult<Vec<MessageItem>> {
	let mut c = ParseCursor::new(input);
	c.is(symbol, '{')?;

	let items = c.many(|input| {
		alt!(input,
			map(field, |v| Some(MessageItem::Field(v))),
			map(enum_, |v| Some(MessageItem::Enum(v))),
			map(message, |v| Some(MessageItem::Message(v))),
			map(extend, |v| Some(MessageItem::Extend(v))),
			map(extensions, |v| Some(MessageItem::Extensions(v))),
			map(oneof, |v| Some(MessageItem::OneOf(v))),
			map(mapField, |v| Some(MessageItem::MapField(v))),
			map(reserved, |v| Some(MessageItem::Reserved(v))),
			map(emptyStatement, |v| None))
	}).into_iter().filter_map(|x| x).collect::<Vec<_>>();

	c.is(symbol, '}')?;
	c.unwrap_with(items)
}

// Proto 2
// extend = "extend" messageType "{" {field | group | emptyStatement} "}"
fn extend(input: &[Token]) -> ParseResult<Extend> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "extend")?;
	let typ = c.next(messageType)?;
	c.is(symbol, '{')?;
	let body = c.many(|input| {
		let mut c = ParseCursor::new(input);
		let item = c.next(field).map(|f| Some(ExtendItem::Field(f)))
			.or_else(|_| c.next(group).map(|g| Some(ExtendItem::Group(g))))
			.or_else(|_| c.next(emptyStatement).map(|_| None))?;
		c.unwrap_with(item)
	}).into_iter().filter_map(|x| x).collect::<Vec<_>>();
	c.is(symbol, '}')?;
	c.unwrap_with(Extend { typ, body })
}

// TODO: Proto 3 has no 'stream'
// service = "service" serviceName "{" { option | rpc | stream | emptyStatement } "}"
fn service(input: &[Token]) -> ParseResult<Service> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "service")?;
	let name = c.next(serviceName)?;
	c.is(symbol, '{')?;
	let body = c.many(|input| {
		alt!(input,
			map(option, |v| Some(ServiceItem::Option(v))),
			map(rpc, |v| Some(ServiceItem::RPC(v))),
			map(stream, |v| Some(ServiceItem::Stream(v))),
			map(emptyStatement, |_| None)
		)
	}).into_iter().filter_map(|x| x).collect::<Vec<_>>();
	c.is(symbol, '}')?;
	c.unwrap_with(Service { name, body })
}

// ( "{" { option | emptyStatement } "}" ) | ";"
fn options_body(input: &[Token]) -> ParseResult<Vec<Opt>> {
	let mut c = ParseCursor::new(input);

	let options_parser = |input| {
		let mut c = ParseCursor::new(input);
		c.is(symbol, '{')?;
		let opts = c.many(|input| {
			let mut c = ParseCursor::new(input);
			let item = c.next(option).map(|o| Some(o))
				.or_else(|_| c.next(emptyStatement).map(|_| None))?;
			c.unwrap_with(item)
		}).into_iter().filter_map(|x| x).collect::<Vec<_>>();
		c.is(symbol, '}')?;
		c.unwrap_with(opts)
	};

	let options = c.next(options_parser)
		.or_else(|_| c.is(symbol, ';').map(|_| vec![]))?;

	c.unwrap_with(options)
}

// rpc = "rpc" rpcName "(" [ "stream" ] messageType ")" "returns" "(" [ "stream" ]
//       messageType ")" (( "{" { option | emptyStatement } "}" ) | ";" )
fn rpc(input: &[Token]) -> ParseResult<RPC> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "rpc")?;
	let name = c.next(rpcName)?;
	c.is(symbol, '(')?;

	let is_stream = |c: &mut ParseCursor| {
		c.is(ident, "stream").map(|_| true).unwrap_or(false)
	};

	let req_stream = is_stream(&mut c);
	let req_type = c.next(messageType)?;
	c.is(symbol, ')')?;
	c.is(ident, "returns")?;
	c.is(symbol, '(')?;

	let res_stream = is_stream(&mut c);
	let res_type = c.next(messageType)?;
	c.is(symbol, ')')?;

	let options = c.next(options_body)?;

	c.unwrap_with(RPC { name, req_type, req_stream, res_type, res_stream, options })
}

// Proto 2 only
// stream = "stream" streamName "(" messageType "," messageType ")" (( "{"
// { option | emptyStatement } "}") | ";" )
fn stream(input: &[Token]) -> ParseResult<Stream> {
	let mut c = ParseCursor::new(input);
	c.is(ident, "stream")?;
	let name = c.next(streamName)?;
	c.is(symbol, '(')?;
	let input_type = c.next(messageType)?;
	c.is(symbol, ',')?;
	let output_type = c.next(messageType)?;
	c.is(symbol, ')')?;
	let options = c.next(options_body)?;
	c.unwrap_with(Stream { name, input_type, output_type, options })
}

pub enum ProtoItem {
	Import(Import),
	Option(Opt),
	Package(String),
	TopLevelDef(TopLevelDef),
	None
}

// Proto 2 and 3
// proto = syntax { import | package | option | topLevelDef | emptyStatement }
pub fn proto(input: &[Token]) -> ParseResult<Proto> {
	let mut c = ParseCursor::new(input);
	let s = c.next(syntax)?;
	// TODO: If no syntax is available, default to proto 2
	let body = c.many(|input| {
		alt!(input,
			map(import, |v| ProtoItem::Import(v)),
			map(package, |v| ProtoItem::Package(v)),
			map(option, |v| ProtoItem::Option(v)),
			map(topLevelDef, |v| ProtoItem::TopLevelDef(v)),
			map(emptyStatement, |v| ProtoItem::None))
	});

	let mut p = Proto {
		syntax: s,
		package: String::new(),
		imports: vec![],
		options: vec![],
		definitions: vec![]
	};

	let mut has_package = false;
	for item in body.into_iter() {
		match item {
			ProtoItem::Import(i) => { p.imports.push(i); },
			ProtoItem::Option(o) => { p.options.push(o); },
			ProtoItem::Package(s) => {
				// A proto file should only up to one package declaraction.
				if has_package {
					return Err(ParseError::Failure);
				}

				has_package = true;
				p.package = s;
			},
			ProtoItem::TopLevelDef(d) => { p.definitions.push(d); },
			ProtoItem::None => {}
		};
	}

	// TODO: Should now be at the end of the file
	c.unwrap_with(p)
}

// TODO: Proto3 has no extend
// topLevelDef = message | enum | extend | service
fn topLevelDef(input: &[Token]) -> ParseResult<TopLevelDef> {
	alt!(input,
		map(message, |m| TopLevelDef::Message(m)),
		map(enum_, |e| TopLevelDef::Enum(e)),
		map(extend, |e| TopLevelDef::Extend(e)),
		map(service, |s| TopLevelDef::Service(s))
	)
}
