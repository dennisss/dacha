// Tokenizer for .proto files
//
// This parallels the 'lexical elements' described here:
// https://developers.google.com/protocol-buffers/docs/reference/proto2-spec
// (when applicable, comments have been left to refer to the original grammar
//  lines referenced in that site).
//
// This is step one in parsing a .proto file and deals with whitespace,
// comments, quoted strings, identifiers, etc. This tokenizer is shared because
// proto2 and proto3 files which only differ in their higher level parser
// implemented on top of tokens.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use common::errors::*;
use parsing::*;

// TODO: Implement parser on ascii strings to make more efficient indexing?

#[derive(Debug, PartialEq)]
pub enum Token {
    Whitespace,
    Comment,
    Identifier(String),
    Integer(u64),
    Float(f64),
    String(Vec<u8>),
    Symbol(char),
}

impl Token {
    parser!(pub parse<&str, Self> => alt!(
        whitespace, comment,
        map(ident, |s| Self::Identifier(s)),
        map(int_lit, |i| Self::Integer(i)),
        map(float_lit, |f| Self::Float(f)),
        map(strLit, |s| Self::String(s)),
        symbol
    ));

    parser!(pub parse_filtered<&str, Self> => seq!(c => {
        c.next(Self::parse_padding)?;
        c.next(Self::parse)
    }));

    parser!(pub parse_padding<&str, ()> => seq!(c => {
        c.next(many(and_then(Self::parse, |tok| {
            match tok {
                Self::Whitespace | Self::Comment => Ok(()),
                _ => Err(err_msg("Not whitespace/comment"))
            }
        })))?;

        Ok(())
    }));
}

// letter = "A" … "Z" | "a" … "z"
pub fn letter(c: char) -> bool {
    c.is_alphabetic()
}
// capitalLetter =  "A" … "Z"
pub fn capital_letter(c: char) -> bool {
    c.is_uppercase() && letter(c)
}
// decimalDigit = "0" … "9"
pub fn decimal_digit(c: char) -> bool {
    c.is_digit(10)
}
// octalDigit   = "0" … "7"
pub fn octal_digit(c: char) -> bool {
    c.is_digit(8)
}
// hexDigit     = "0" … "9" | "A" … "F" | "a" … "f"
pub fn hex_digit(c: char) -> bool {
    c.is_ascii_hexdigit()
}

// NOTE: Only public to be used in the textproto format.
// ident = letter { letter | decimalDigit | "_" }
parser!(pub ident<&str, String> => {
    map(slice(seq!(c => {
        c.next(like(|c: char| { c.is_alphabetic() || c == '_' }))?;
        c.next(take_while(|c: char| {
            letter(c) || decimal_digit(c) || c == '_'
        }))?;
        Ok(())
    })), |s: &str| s.to_owned())
});

// NOTE: Only public to be used in the textproto format.
// intLit = decimalLit | octalLit | hexLit
parser!(pub int_lit<&str, u64> => alt!(
    // NOTE: decimal_lit must be after hex_lit as overlaps with decimal_lit.
    hex_lit, octal_lit, binary_lit, decimal_lit
));

// decimalLit = ( "1" … "9" ) { decimalDigit }
parser!(decimal_lit<&str, u64> => seq!(c => {
    c.next(peek(like(|c| c != '0')))?;
    let digits = c.next(take_while1(|v| decimal_digit(v)))?;

    Ok(u64::from_str_radix(digits, 10).unwrap())
}));

// octalLit   = "0" { octalDigit }
parser!(octal_lit<&str, u64> => seq!(c => {
    c.next(tag("0"))?;
    let digits = c.next(take_while(|v| octal_digit(v as char)))?;
    Ok(u64::from_str_radix(digits, 8).unwrap_or(0))
}));

// hexLit     = "0" ( "x" | "X" ) hexDigit { hexDigit }
parser!(hex_lit<&str, u64> => seq!(c => {
    c.next(tag("0"))?;
    c.next(one_of("xX"))?;
    let digits = c.next(take_while1(|v| hex_digit(v)))?;
    Ok(u64::from_str_radix(digits, 16).unwrap())
}));

// NOTE: Not standard in the protobuf spec
parser!(binary_lit<&str, u64> => seq!(c => {
    c.next(tag("0b"))?;
    let digits = c.next(take_while1(|v| v == '0' || v == '1'))?;
    Ok(u64::from_str_radix(digits, 2).unwrap())
}));

// TODO: Is this allowed to start with a '0' character?
// floatLit = ( decimals "." [ decimals ] [ exponent ] | decimals exponent |
// "."decimals [ exponent ] ) | "inf" | "nan"
parser!(pub float_lit<&str, f64> => alt!(
    seq!(c => {
        let a = c.next(decimals)?;
        c.next(tag("."))?;
        let b = c.next(opt(decimals))?;
        let e = c.next(opt(exponent))?;

        let combined = String::from(a) + "." + &b.unwrap_or(String::from("0")) + "e"
        + e.unwrap_or(String::from("0")).as_str();

        Ok(combined.as_str().parse::<f64>().unwrap())
    }),

    map(tag("inf"), |_| std::f64::INFINITY), // < Negative infinity?
    map(tag("nan"), |_| std::f64::NAN)
));

// decimals = decimalDigit { decimalDigit }
parser!(decimals<&str, String> => map(
	take_while1(|c| decimal_digit(c)),
	|s: &str| s.to_owned()));

// exponent = ( "e" | "E" ) [ "+" | "-" ] decimals
parser!(exponent<&str, String> => seq!(c => {
    c.next(one_of("eE"))?;
    let sign = c.next(one_of("+-"))? as char;
    let num = c.next(decimals)?;
    let mut s = String::new();
    s.push(sign);
    Ok(s + &num)
}));

// strLit = ( "'" { charValue } "'" ) | ( '"' { charValue } '"' )
// charValue = hexEscape | octEscape | charEscape | /[^\0\n\\]/
//
// TODO: Also support "\uXXXX" which is used in the text format to represent a
// unicode code point rather than just one byte.
parser!(pub strLit<&str, Vec<u8>> => seq!(c => {
    let mut out = vec![];

    let q = c.next(quote)?;

    loop {
        if let Some(byte) = c.next(opt(alt!(hex_escape, oct_escape, char_escape)))? {
            out.push(byte);
        } else if let Some(c) = c.next(opt(like(|c| c != q && c != '\0' && c != '\n' && c != '\\')))? {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            out.extend_from_slice(s.as_bytes());
        } else {
            break;
        }
    }

    c.next(atom(q))?;

    Ok(out)
}));

pub fn serialize_str_lit(value: &[u8], out: &mut String) {
    out.push('"');
    for b in value.iter().cloned() {
        if b != b'\\'
            && b != b'"'
            && (b.is_ascii_alphanumeric() || b == b' ' || b.is_ascii_punctuation())
        {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\x{:02x}", b));
        }
    }
    out.push('"');
}

// hexEscape = '\' ( "x" | "X" ) hexDigit hexDigit
parser!(hex_escape<&str, u8> => seq!(c => {
    c.next(tag("\\"))?;
    c.next(one_of("xX"))?;
    let digits = c.next(take_exact::<&str>(2))?;
    for c in digits.chars() {
        if !hex_digit(c) {
            return Err(err_msg("Expected hex digit"));
        }
    }

    Ok(u8::from_str_radix(digits, 16).unwrap())
}));
//do_parse!(
//	char!('\\') >> one_of!("xX") >> digits: take_while_m_n!(2, 2, hexDigit) >>
//	(u8::from_str_radix(digits, 16).unwrap() as char)
//));

// TODO: It is possible for this to go out of bounds.
// octEscape = '\' octalDigit octalDigit octalDigit
parser!(oct_escape<&str, u8> => seq!(c => {
    c.next(tag("\\"))?;
    let digits = c.next(take_exact::<&str>(3))?; // TODO: Use 'n_like'
    for c in digits.chars() {
        if !octal_digit(c) {
            return Err(err_msg("Not an octal digit"));
        }
    }

    Ok(u8::from_str_radix(digits, 8).unwrap())
}));

// charEscape = '\' ( "a" | "b" | "f" | "n" | "r" | "t" | "v" | '\' | "'" | '"'
// )
parser!(char_escape<&str, u8> => seq!(c => {
    c.next(tag("\\"))?;
    let c = c.next(one_of("abfnrtv\\'\""))?;
    Ok(match c {
        'a' => b'\x07',
        'b' => b'\x08',
        'f' => b'\x0c',
        'n' => b'\n',
        'r' => b'\r',
        't' => b'\t',
        c => c as u8
    })
}));

// quote = "'" | '"'
parser!(quote<&str, char> => map(one_of("\"'"), |v| v as char));

// Below here, none of these are in the online spec but are implemented by
// the standard protobuf tokenizer.

parser!(whitespace<&str, Token> => map(
    take_while1(|c: char| c.is_whitespace()),
    |_| Token::Whitespace
));

parser!(line_comment<&str, Token> => seq!(c => {
    c.next(tag("//"))?;
    c.next(take_while(|c| c != '\n'))?;
    Ok(Token::Comment)
}));

parser!(block_comment<&str, Token> => seq!(c => {
    c.next(tag("/*"))?;
    c.next(take_until(tag("*/")))?;
    c.next(tag("*/"))?;
    Ok(Token::Comment)
}));

parser!(comment<&str, Token> => alt!(
    line_comment, block_comment
));

parser!(symbol<&str, Token> => seq!(c => {
//	let c = c.next(like(|c: char| {
//		// '/' is only used for comments. Also must be printable but not used for anything else
//		c != '/' && !c.is_alphanumeric()
//	}))?;

    let c = c.next(one_of(".;+-=[],{}<>()"))?;

    Ok(Token::Symbol(c))
}));

// Now we can trivially implement a tokenizer that simply iteratively tries to
// get more tokens
