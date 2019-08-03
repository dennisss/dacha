// Tokenizer for .proto files
//
// This parallels the 'lexical elements' described here:
// https://developers.google.com/protocol-buffers/docs/reference/proto2-spec
// (when applicable, comments have been left to refer to the original grammar
//  lines referenced in that site).
//
// This is step one in parsing a .proto file and deals with whitespace, comments,
// quoted strings, identifiers, etc. This tokenizer is shared because proto2 and
// proto3 files which only differ in their higher level parser implemented on top
// of tokens. 


// TODO: Implement parser on ascii strings to make more efficient indexing?

#[derive(Debug, PartialEq)]
pub enum Token {
	Whitespace,
	Comment,
	Identifier(String),
	Integer(usize),
	Float(f64),
	String(String),
	Symbol(char),
	Done // End of file
}

pub struct Tokenizer<'a> {
	data: &'a str
}

impl Tokenizer<'_> {
	pub fn new(data: &str) -> Tokenizer {
		Tokenizer { data }
	}

	pub fn next(&mut self) -> Option<Token> {
		// TODO: May also fail

		match token(self.data) {
			Ok((rest, tok)) => {
				self.data = rest;
				return Some(tok);
			},
			_ => {
				// Should return eof if all done
				return None;
			}
		};
	}
}


// letter = "A" … "Z" | "a" … "z"
pub fn letter(c: char) -> bool { c.is_alphabetic() }
// capitalLetter =  "A" … "Z"
pub fn capitalLetter(c: char) -> bool { c.is_uppercase() && letter(c) }
// decimalDigit = "0" … "9"
pub fn decimalDigit(c: char) -> bool { c.is_digit(10) }
// octalDigit   = "0" … "7"
pub fn octalDigit(c: char) -> bool { c.is_digit(8) }
// hexDigit     = "0" … "9" | "A" … "F" | "a" … "f"
pub fn hexDigit(c: char) -> bool { c.is_ascii_hexdigit() }

// ident = letter { letter | decimalDigit | "_" }
named!(ident<&str, String>, do_parse!(
	head: take_while_m_n!(1, 1, |c: char|
		c.is_alphabetic() || c == '_'
	) >>
	rest: take_while!(|c: char| {
		letter(c) || decimalDigit(c) || c == '_'
	}) >>
	(String::from(head) + rest)
));


// intLit     = decimalLit | octalLit | hexLit
named!(intLit<&str, usize>, alt!(
	decimalLit | octalLit | hexLit
));

// decimalLit = ( "1" … "9" ) { decimalDigit }
named!(decimalLit<&str, usize>, do_parse!(
	peek!(take_while_m_n!(1, 1, |c: char| c != '0')) >>
	digits: take_while1!(decimalDigit) >>
	(usize::from_str_radix(digits, 10).unwrap())
));

// octalLit   = "0" { octalDigit }
named!(octalLit<&str, usize>, do_parse!(
	char!('0') >>
	digits: take_while1!(octalDigit) >>
	(usize::from_str_radix(digits, 8).unwrap_or(0))
));


// hexLit     = "0" ( "x" | "X" ) hexDigit { hexDigit } 
named!(hexLit<&str, usize>, do_parse!(
	char!('0') >> one_of!("xX") >>
	digits: take_while1!(hexDigit) >>
	(usize::from_str_radix(digits, 16).unwrap())
));

// TODO: Is this allowed to start with a '0' character?
// floatLit = ( decimals "." [ decimals ] [ exponent ] | decimals exponent | "."decimals [ exponent ] ) | "inf" | "nan"
named!(floatLit<&str, f64>, alt!(
	do_parse!(
		a: decimals >> char!('.') >> b: opt!(decimals) >> e: opt!(exponent) >>
		((String::from(a) + "." + b.unwrap_or("0") + "e" + e.unwrap_or(String::new()).as_str()).as_str().parse::<f64>().unwrap())
	) |

	map!(tag!("inf"), |_| std::f64::INFINITY) | // < Negative infinity?
	map!(tag!("nan"), |_| std::f64::NAN)
));

// decimals  = decimalDigit { decimalDigit }
named!(decimals<&str, &str>, take_while1!(decimalDigit));

// exponent  = ( "e" | "E" ) [ "+" | "-" ] decimals 
named!(exponent<&str, String>, do_parse!(
	one_of!("eE") >>
	sign: one_of!("+-") >>
	num: decimals >>
	({ let mut s = String::new(); s.push(sign); s + num })
));

// strLit = ( "'" { charValue } "'" ) | ( '"' { charValue } '"' )
named!(strLit<&str, String>, alt!(
	do_parse!(
		q: quote >>
		val: many0!(charValue) >>
		char!(q) >>
		({
			let mut s = String::new();
			for c in val {
				s.push(c);
			}

			s
		})
	)
));

// charValue = hexEscape | octEscape | charEscape | /[^\0\n\\]/
named!(charValue<&str, char>, alt!(
	hexEscape | octEscape | charEscape |

	// NOTE: Can't be '"' because of strLit
	map!(take_while_m_n!(1, 1, |c: char| c != '"' && c != '\0' && c != '\n' && c != '\\'), |s| s.chars().next().unwrap())
));

// hexEscape = '\' ( "x" | "X" ) hexDigit hexDigit
named!(hexEscape<&str, char>, do_parse!(
	char!('\\') >> one_of!("xX") >> digits: take_while_m_n!(2, 2, hexDigit) >>
	(u8::from_str_radix(digits, 16).unwrap() as char)
));

// TODO: It is possible for this to go out of bounds.
// octEscape = '\' octalDigit octalDigit octalDigit
named!(octEscape<&str, char>, do_parse!(
	char!('\\') >> digits: take_while_m_n!(3, 3, octalDigit) >>
	(u8::from_str_radix(digits, 8).unwrap() as char)
));

// charEscape = '\' ( "a" | "b" | "f" | "n" | "r" | "t" | "v" | '\' | "'" | '"' )
named!(charEscape<&str, char>, do_parse!(
	char!('\\') >> c: one_of!("abfnrtv\\'\"") >>
	(match c {
		'a' => '\x07',
		'b' => '\x08',
		'f' => '\x0c',
		'n' => '\n',
		'r' => '\r',
		't' => '\t',
		c => c
	})
));

// quote = "'" | '"'
named!(quote<&str, char>, one_of!("\"'"));


/// Below here, none of these are in the online spec but are implemented by
/// the standard protobuf tokenizer.

named!(whitespace<&str, Token>, map!(
	take_while1!(|c: char| c.is_whitespace()),
	|_| Token::Whitespace
));

named!(lineComment<&str, Token>, do_parse!(
	tag!("//") >>
	take_while!(|c| c != '\n') >>
	(Token::Comment)
));

named!(blockComment<&str, Token>, do_parse!(
	tag!("/*") >> take_until!("*/") >> tag!("*/") >>
	(Token::Comment)
));

named!(comment<&str, Token>, alt!(
	lineComment | blockComment
));

named!(symbol<&str, Token>, do_parse!(
	c: take_while_m_n!(1, 1, |c: char| {
		// '/' is only used for comments. Also must be printable but not used for anything else
		c != '/' && !c.is_alphanumeric()
	}) >>
	(Token::Symbol(c.chars().next().unwrap()))
));

named!(token<&str, Token>, alt!(
	whitespace | comment |
	map!(ident, |s| Token::Identifier(s)) |
	map!(intLit, |i| Token::Integer(i)) |
	map!(floatLit, |f| Token::Float(f)) |
	map!(strLit, |s| Token::String(s)) |
	symbol
));




// Now we can trivially implement a tokenizer that simply iteratively tries to get more tokens


