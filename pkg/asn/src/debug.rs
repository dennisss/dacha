// This file contains an implementation of a human readable debug format for
// viewing DER serialized messages.

use super::tag::*;
use super::encoding::*;
use super::builtin::*;
use math::big::BigInt;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use common::bits::BitVector;
use common::errors::*;
use parsing::*;
use crate::debug::ParsedElementValue::Unknown;

#[derive(Debug)]
pub struct ParsedElement {
	tag: Tag,
	value: ParsedElementValue
}

impl ParsedElement {
	fn parse(input: Bytes) -> ParseResult<Self> {
		let (el, rest) = Element::parse(input)?;

		let value =
			if el.ident.constructed {
				ParsedElementValue::Constructed(Self::parse_many(el.data)?)
			} else if el.ident.tag.class == TagClass::Universal {
				// TODO: On any value parsing failures here, just convert it
				// to an Unknown instead of failing.
				match el.ident.tag.number {
					TAG_NUMBER_BOOLEAN => {
						ParsedElementValue::Bool(bool::from_der(el.outer)?)
					},
					TAG_NUMBER_INTEGER => {
						ParsedElementValue::Int(BigInt::from_der(el.outer)?)
					},
					TAG_NUMBER_NULL => {
						Null::from_der(el.outer)?;
						ParsedElementValue::Null
					},
					TAG_NUMBER_BIT_STRING => {
						ParsedElementValue::Bits(
							BitString::from_der(el.outer)?.into())
					},
					TAG_NUMBER_OCTET_STRING => {
						ParsedElementValue::Binary(
							OctetString::from_der(el.outer)?.into_bytes())
					},
					TAG_NUMBER_UTF8STRING => {
						ParsedElementValue::String(
							UTF8String::from_der(el.outer)?.to_string())
					},
					TAG_NUMBER_OBJECT_IDENTIFIER => {
						ParsedElementValue::ObjectIdentifier(
							ObjectIdentifier::from_der(el.outer)?)
					},

					_ => Unknown(el.data)
				}
			} else {
				ParsedElementValue::Unknown(el.data)
			};

		Ok((Self { tag: el.ident.tag, value }, rest))
	}

	fn parse_many(input: Bytes) -> Result<Vec<Self>> {
		let (arr, _) = complete(many(Self::parse))(input)?;
		Ok(arr)
	}
}

#[derive(Debug)]
pub enum ParsedElementValue {
	Constructed(Vec<ParsedElement>),
	String(String),
	Int(BigInt),
	ObjectIdentifier(ObjectIdentifier),
	Binary(Bytes),
	Bits(BitVector),
	Bool(bool),
	Null,
	Real(f64),
	Enum(BigInt),
	Time(DateTime<Utc>),
	Unknown(Bytes)
}

pub fn print_debug_string(input: Bytes) {
	let el = ParsedElement::parse_many(input).unwrap();
	println!("{:#?}", el);
}
