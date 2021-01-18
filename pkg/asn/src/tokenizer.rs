use common::bits::BitVector;
use common::bytes::{Buf, Bytes};
use common::errors::*;
use parsing::ascii::AsciiString;
use parsing::*;

// https://asn1.io/asn1playground/
// See T-REC-X.680-201508-I!!PDF-E.pdf

const RESERVED_WORDS: &'static [&'static [u8]] = &[
    b"ABSENT",
    b"ABSTRACT-SYNTAX",
    b"ALL",
    b"APPLICATION",
    b"AUTOMATIC",
    b"BEGIN",
    b"BIT",
    b"BMPString",
    b"BOOLEAN",
    b"BY",
    b"CHARACTER",
    b"CHOICE",
    b"CLASS",
    b"COMPONENT",
    b"COMPONENTS",
    b"CONSTRAINED",
    b"CONTAINING",
    b"DATE",
    b"DATE-TIME",
    b"DEFAULT",
    b"DEFINITIONS",
    b"DURATION",
    b"EMBEDDED",
    b"ENCODED",
    b"ENCODING-CONTROL",
    b"END",
    b"ENUMERATED",
    b"EXCEPT",
    b"EXPLICIT",
    b"EXPORTS",
    b"EXTENSIBILITY",
    b"EXTERNAL",
    b"FALSE",
    b"FROM",
    b"GeneralizedTime",
    b"GeneralString",
    b"GraphicString",
    b"IA5String",
    b"IDENTIFIER",
    b"IMPLICIT",
    b"IMPLIED",
    b"IMPORTS",
    b"INCLUDES",
    b"INSTANCE",
    b"INSTRUCTIONS",
    b"INTEGER",
    b"INTERSECTION",
    b"ISO646String",
    b"MAX",
    b"MIN",
    b"MINUS-INFINITY",
    b"NOT-A-NUMBER",
    b"NULL",
    b"NumericString",
    b"OBJECT",
    b"ObjectDescriptor",
    b"OCTET",
    b"OF",
    b"OID-IRI",
    b"OPTIONAL",
    b"PATTERN",
    b"PDV",
    b"PLUS-INFINITY",
    b"PRESENT",
    b"PrintableString",
    b"PRIVATE",
    b"REAL",
    b"RELATIVE-OID",
    b"RELATIVE-OID-IRI",
    b"SEQUENCE",
    b"SET",
    b"SETTINGS",
    b"SIZE",
    b"STRING",
    b"SYNTAX",
    b"T61String",
    b"TAGS",
    b"TeletexString",
    b"TIME",
    b"TIME-OF-DAY",
    b"TRUE",
    b"TYPE-IDENTIFIER",
    b"UNION",
    b"UNIQUE",
    b"UNIVERSAL",
    b"UniversalString",
    b"UTCTime",
    b"UTF8String",
    b"VideotexString",
    b"VisibleString",
    b"WITH",
    // Deprecated words
    b"ANY",
    b"DEFINED",
    b"BY",
];

/// Sorted in descending order of length when there is an ambiguity.
const CHAR_SEQUENCES: &'static [&'static [u8]] =
    &[b"::=", b"...", b"..", b"[[", b"]]", b"</", b"/>"];

// TODO: Rename THis symbols?
/// Each byte is this is a separate token.
/// TODO: Should quotation marks ever appear outside of a string. Also probably
/// Ommited compared to the spec "\"' ".
/// TODO: Maybe also remove '.' as that could just be for realnumbers?
const CHAR_LITERALS: &'static [u8] = b"{}<>,./()[]-:=;@|!^";

//pub fn one_of<I: CharIter + ParserFeed>(s: &'static str) -> impl Parser<char,
// I> 	where I: ParserFeed<Item=char> {
//	like(move |i| is_one_of(s, i))
//}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {}

impl Token {
    parser!(pub whitespace<()> => map(one_of(b" \r\n\t"), |_| ()));

    parser!(pub comment<Bytes> => {
        alt!(
            seq!(c => {
                c.next(tag("--"))?;
                // TODO: Accept any type of new line
                let end_marker = alt!(
                    map(tag("--"), |_| ()), map(one_of(b"\n"), |_| ())
                );

                let inner = c.next(take_until(end_marker))?;

                // This should always succeed.
                c.next(end_marker)?;
                Ok(inner)
            }),
            seq!(c => {
                c.next(tag("/*"))?;
                let inner = c.next(take_until(tag("*/")))?;
                c.next(tag("*/"))?;
                Ok(inner)
            })
        )
    });

    parser!(pub reserved<AsciiString> => {
        map(anytag(RESERVED_WORDS), |w| unsafe {
            AsciiString::from_ascii_unchecked(w) })
    });

    parser!(pub sequence<AsciiString> => {
        map(anytag(CHAR_SEQUENCES), |w| unsafe {
            AsciiString::from_ascii_unchecked(w) })
    });

    parser!(pub symbol<u8> => one_of(CHAR_LITERALS));

    // '(0|[1-9][0-9]+)'
    parser!(pub number<usize> => {
        and_then(take_while1(|v| (v as char).is_digit(10)), |v: Bytes| {
            if v[0] == ('0' as u8) && v.len() != 1 {
                return Err(err_msg("Unexpected leading zero "));
            }

            // We should have only parsed digits so the UTF-8 parsing should never
            // fail, but the number could be too large to fit into an integer.
            let v = usize::from_str_radix(
                &String::from_utf8(v.to_vec()).unwrap(), 10)?;

            Ok(v)
        })
    });

    parser!(pub realnumber<f64> => {
        seq!(c => {
            let int = c.next(Self::number)?;

            // TODO:
            // let frac = c.next(opt(seq!(c => {
            // 	c.next(symbol('.'))?;
            // 	let n = c.next()
            // })))

            Ok(int as f64)
        })
    });

    parser!(pub bstring<BitVector> => seq!(c => {
        c.next(one_of(b"'"))?;
        let mut out = BitVector::new();

        loop {
            c.next(opt(many(Self::whitespace)))?;
            let bit = c.next(opt(one_of(b"01")))?;
            if let Some(b) = bit {
                out.push(if b == ('0' as u8) { 0 } else { 1 });
            } else {
                break;
            }
        }

        c.next(tag("'B"))?;
        Ok(out)
    }));

    parser!(pub hstring<Bytes> => seq!(c => {
        c.next(one_of(b"'"))?;
        let mut out = String::new();

        loop {
            c.next(opt(many(Self::whitespace)))?;
            let hexchar = c.next(opt(one_of(b"0123456789ABCDEF")))?;
            if let Some(h) = hexchar {
                out.push(h as char);
            } else {
                break;
            }
        }

        c.next(tag("'H"))?;
        // TODO: This doesn't support strings with an un-even number of hex
        // characters
        let buf = hex::decode(&out)?;
        Ok(buf.into())
    }));

    // '[A-Z](\-?[a-Z0-9])*'
    parser!(pub typereference<AsciiString> => {
        map(slice(seq!(c => {
            let first = c.next(any)?;
            if !(first as char).is_ascii_uppercase() || !is_alpha(first) {
                return Err(err_msg("First must be uppercase"));
            }

            c.next(many(seq!(c => {
                c.next(opt(one_of(b"-")))?;
                let sym = c.next(any)? as char;
                if !sym.is_ascii_alphanumeric() {
                    return Err(err_msg("Expected alphanumeric"));
                }
                Ok(())
            })))?;

            Ok(())
        })), |v| unsafe { AsciiString::from_ascii_unchecked(v) })
    });

    // TODO: Same thing as typereference except diffent first character
    // capitalization
    parser!(pub identifier<AsciiString> => {
        map(slice(seq!(c => {
            let first = c.next(any)?;
            if (first as char).is_ascii_uppercase() || !is_alpha(first) {
                return Err(err_msg("First must be lowercase"));
            }

            c.next(many(seq!(c => {
                c.next(opt(one_of(b"-")))?;
                let sym = c.next(any)? as char;
                if !sym.is_ascii_alphanumeric() {
                    return Err(err_msg("Expected alphanumeric"));
                }
                Ok(())
            })))?;

            Ok(())
        })), |v| unsafe { AsciiString::from_ascii_unchecked(v) })
    });

    // TODO: simplestring can only contain ASCII visible characters and can't
    // contain an escaped quotation mark. cstring can be unicode
    // TODO: Should double check unicode support in the spec.
    pub fn parse_string(mut input: Bytes) -> ParseResult<Bytes> {
        // TODO: Currently this doesn't support unicode parsing.
        if input.len() < 2 {
            return Err(err_msg("Too short for string"));
        }

        if input[0] != '"' as u8 {
            return Err(err_msg("Bad delimiter"));
        }

        input.advance(1);

        let mut data = vec![];

        let mut i = 0;
        while i < input.len() {
            // TODO: Unescape the quote
            if input[i] == '"' as u8 {
                // Escaped quote
                if (i < input.len() - 1 && input[i + 1] == '"' as u8) {
                    data.push(input[i]);
                    i += 2;
                    continue;
                }

                input.advance(i + 1);
                return Ok((data.into(), input));
            } else {
                data.push(input[i]);
                i += 1;
            }
        }

        Err(err_msg("Unterminated string"))
    }

    pub fn skip_to<T, P: Parser<T>>(p: P) -> impl Parser<T> {
        then(many(alt!(Self::whitespace, map(Self::comment, |_| ()))), p)
    }

    // parser!(parse_token<Token> => {
    // 	alt!(
    // 		parse_whitespace, parse_comment, parse_reserved, parse_sequence,
    // 		parse_reference, parse_identifier,

    // 		// realnumber
    // 		parse_number,

    // 		parse_string,

    // 		// Should generally always be the last one
    // 		parse_symbol
    // 	)
    // });
}

fn is_alpha(v: u8) -> bool {
    (v as char).is_alphabetic()
}

// Taken from the ANS.1 playground.
const TEST_SCHEMA: &'static str = r#"
World-Schema DEFINITIONS AUTOMATIC TAGS ::= 
BEGIN
  Rocket ::= SEQUENCE       
  {                                                     
     name      UTF8String (SIZE(1..16)),
     message   UTF8String DEFAULT "Hello World" , 
     fuel      ENUMERATED {solid, liquid, gas},
     speed     CHOICE     
     { 
        mph    INTEGER,  
        kmph   INTEGER  
     }  OPTIONAL, 
     payload   SEQUENCE OF UTF8String 
  }                                                     
END
"#;

/*
#[derive(Debug)]
struct Rule {
    name: Bytes,
    alts: Vec<Vec<Token>>
}

fn split_syntax(tokens: Vec<Token>) -> Result<Vec<Rule>> {

    let mut rules = vec![];

    let mut i = 0;
    let decl = Token::Sequence("::=".into());

    while i < tokens.len() {
        if i + 2 >= tokens.len() {
            return Err(err_msg("Too few tokens to form rule"));
        }

        let name =
            if let Token::Reference(name) = &tokens[i] {
                name.clone()
            } else {
                return Err(err_msg("Expected rule name"));
            };
        i += 1;

        if tokens[i] != decl {
            return Err(err_msg("Expected ::="));
        }
        i += 1;

        let mut body = vec![];

        for j in i..tokens.len() {
            if j + 1 < tokens.len() && tokens[j + 1] == decl {
                break;
            }

            body.push(tokens[j].clone());
        }
        i += body.len();

        let mut alts = vec![ vec![] ];
        for t in body.into_iter() {
            if t == Token::Symbol('|' as u8) {
                alts.push(vec![]);
            } else {
                alts.last_mut().unwrap().push(t);
            }
        }

        rules.push(Rule { name, alts })
    }


    Ok(rules)
}
*/

/*
fn generate_code(rules: Vec<Rule>) -> Result<()> {
    let typemap: std::collections::HashMap<String, String> = map!(
        "identifier" => "Bytes",
        "number" => "usize",
        "realnumber" => "f64",
        "bstring" => "Bytes",
        "xmlbstring" => "Bytes",
        "hstring" => "Bytes",
        "xmlhstring" => "Bytes",
        "cstring" => "Bytes",
        "simplestring" => "Bytes",
        "xmlcstring" => "Bytes",
        "tstring" => "Bytes",
        "xmltstring" => "Bytes",
        "psname" => "Bytes",
        "extended-true" => "bool",
        "extended-false" => "bool"
    );

    let stringify = |v: &Bytes| {
        String::from_utf8(v.to_vec()).unwrap()
    };

    let one_case = |v: Vec<Token>| {
        let mut s = String::new();

        for t in v.into_iter() {
            match t {
                // Maps to a symbol/sequence. No need to type it.
                Token::String(v) => {
                    s += &format!("/* {} */ ", stringify(&v));
                },
                Token::Reference(v) => {
                    s += &stringify(&v);
                    s += &", ";
                },
                Token::Identifier(v) => {
                    let typ = stringify(&v);

                    let mapped = typemap.get(&typ);
                    if let Some(mapped) = mapped {
                        s += &mapped;
                        s += &", ";
                    } else {
                        println!("-- no type: {}", typ);
                    }
                },
                _ => {
                    println!("-- Unimplemented {:?}", t);
                }
            }
        }

        s
    };

    for mut r in rules.into_iter() {
        let name = stringify(&r.name);

        // Type alias
        if r.alts.len() == 1 && r.alts[0].len() == 1 {
            if let Token::Reference(refer) = &r.alts[0][0] {
                println!("pub type {} = {};", name, stringify(&refer));
                continue;
            }
        }

        if r.alts.len() == 1 {
            let inner = one_case(r.alts.pop().unwrap());
            println!("pub struct {}({});", name, inner);
        } else {
            println!("pub enum {} {{", name);

            let mut i = 0;
            for a in r.alts {
                let inner = one_case(a);
                format!("\t{}({})", (('A' as u8) + i) as char, inner);
            }

            i += 1;

            println!("}}");
        }
    }

    Ok(())
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Read;

    // #[test]
    // fn asn1_tokenize_test() {

    // 	// let mut file =
    // std::fs::File::open("/home/dennis/workspace/dacha/pkg/crypto/src/asn/asn.
    // grammar").unwrap(); 	// let mut data = vec![];
    // 	// file.read_to_end(&mut data).unwrap();

    // 	let (tokens, _) =
    // complete(many(parse_token))(Bytes::from(TEST_SCHEMA)).unwrap();

    // 	let filtered_tokens = tokens.into_iter().filter(|t| {
    // 		if let Token::Whitespace = t { false } else { true }
    // 	}).collect::<Vec<_>>();

    // 	/*
    // 	let rules = split_syntax(filtered_tokens).unwrap();
    // 	// for r in rules.iter() {
    // 	// 	println!("{:?}", r);
    // 	// }

    // 	generate_code(rules).unwrap();
    // 	*/

    // 	println!("{:?}", filtered_tokens);
    // }
}

/*

valuereference
    A "valuereference" shall consist of the sequence of characters specified for an "identifier" in 12.3. In analyzing an
    instance of use of this notation, a "valuereference" is distinguished from an "identifier" by the context in which it
    appears.

modulereference
    A "modulereference" shall consist of the sequence of characters specified for a "typereference" in 12.2. In analyzing an
    instance of use of this notation, a "modulereference" is distinguished from a "typereference" by the context in which it
    appears

empty

number
    A "number" shall consist of one or more digits. The first digit shall not be zero unless the "number" is a single digit.
    NOTE – The "number" lexical item is always mapped to an integer value by interpreting it as decimal notation.

realnumber
    A "realnumber" shall consist of an integer part that is a series of one or more digits, and optionally a decimal point (.).
    The decimal point can optionally be followed by a fractional part which is one or more digits. The integer part, decimal
    point or fractional part (whichever is last present) can optionally be followed by an e or E and an optionally-signed
    exponent which is one or more digits. The leading digit of the exponent shall not be zero unless the exponent is a single
    digit

bstring

xmlbstring

hstring

xmlhstring

Double-quoted strings distinguished by context
    cstring
    simplestring

xmlcstring


tstring

xmltstring

psname


encodingreference
    An "encodingreference" shall consist of a sequence of characters as specified for a "typereference" in 12.2, except that
    no lower-case letters shall be included

integerUnicodeLabel
non-integerUnicodeLabel

xmlasn1typename

Based on context:
    "true"
    extended-true
    "false"
    extended-false

    "NaN"
    "INF"

*/
