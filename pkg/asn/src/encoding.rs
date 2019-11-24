use parsing::*;
use parsing::binary::*;
use parsing::ascii::AsciiString;
use bytes::Bytes;
use common::errors::*;
use super::tag::*;
use super::builtin::*;
use std::convert::TryFrom;
use std::fmt::Debug;
use common::bits::BitVector;
use math::big::BigInt;

// Mainly for the compiled code.
pub use super::tag::{Tag, TagClass};

// http://www.zytrax.com/tech/survival/ssl.html#x509
// https://osqa-ask.wireshark.org/questions/62528/server-certificate-packet-format
// https://tools.ietf.org/html/rfc5280
// https://mirage.io/blog/introducing-asn1

// Reference encoder: https://github.com/etingof/pyasn1/blob/master/pyasn1/codec/ber/encoder.py#L20

// BER/DER Format https://en.wikipedia.org/wiki/X.690#Identifier_octets
// ^ Contains a nice table of primitives

// https://asn1.io/asn1playground/

/// We won't support parsing any tag larger than the native integer size.
const MAX_TAG_NUMBER_BITS: usize = std::mem::size_of::<usize>() * 8;

const USIZE_OCTETS: usize = std::mem::size_of::<usize>();


// Parses a varint stored as a big-endian chunks of the 7 lower bits of each
// octet ending in the last octet without the MSB set.
//
// This format is used by the tag number in long form and for the
// ObjectIdentifier component encoding.
//
// NOTE: This only parses the minimal encoding.
parser!(parse_varint_msb_be<usize> => seq!(c => {
	let mut number = 0;

	let mut finished = false;
	for i in 0..(MAX_TAG_NUMBER_BITS / 7) {
		let octet = c.next(be_u8)?;
		let num_part = octet & 0x7f; // Lower 7 bits
		finished |= (octet >> 7) == 0; // Upper 1 bit

		number = number << 7;
		number |= num_part as usize;

		if finished {
			break;
		} else {
			// We will only parse integers encoded minimally meaning the
			// first octet must have a non-zero value otherwise leading zeros
			// have been encoded.
			if num_part == 0 {
				return Err("Last octet contains zero".into());
			}
		}
	}

	if !finished {
		return Err("Tag number overflow integer range".into());
	}

	Ok(number)
}));

// TODO: Deduplicate the varint code with the other crates.
fn serialize_varint_msb_be(mut num: usize, out: &mut Vec<u8>) {
	let mut buf = [0u8; std::mem::size_of::<usize>()];
	let mut i = buf.len() - 1;
	loop {
		let b = (num & 0x7f) as u8;
		num = num >> 7;

		buf[i] |= b;
		if num == 0 {
			break;
		} else {
			buf[i - 1] |= 0x80;
		}

		i -= 1;
	}

	out.extend_from_slice(&buf[i..]);
}


#[derive(Debug, Clone)]
pub struct Identifier {
	pub tag: Tag,
	// If not constructed, then it is a primitive.
	pub constructed: bool
}

impl Identifier {
	parser!(parse<Self> => { seq!(c => {
		let first = c.next(be_u8)?;
		let class = TagClass::from((first >> 6) & 0b11);
		let constructed = ((first >> 5) & 0b1) == 1;
		let mut number = (first & 0b11111) as usize;

		// In this case, the tag takes 2+ octets.
		if number == 31 {
			number = c.next(parse_varint_msb_be)?;

			// The 2+ octet form should only be used for numbers >= 31
			if number <= 30 {
				return Err("Should have used single octet".into());
			}
		}

		Ok(Self { tag: Tag { class, number }, constructed })
	}) });

	fn serialize(&self, out: &mut Vec<u8>) {
		let first = ((self.tag.class as u8) << 6)
			| ((if self.constructed { 1 } else { 0 }) << 5)
			| (if self.tag.number <= 30 { self.tag.number as u8 } else { 31 });
		out.push(first);

		if self.tag.number >= 31 {
			serialize_varint_msb_be(self.tag.number, out);
		}
	}
}

#[derive(Debug, Clone)]
pub enum Length {
	Short(u8),
	Long(usize),
	Indefinite
}

impl Length {
	parser!(parse<Self> => { seq!(c => {
		let first = c.next(be_u8)?;
		let upper = first & 0x80;
		let lower = first & 0x7f;
		if upper == 0 {
			Ok(Self::Short(lower))
		} else {
			if lower == 0 {
				return Ok(Self::Indefinite);
			}
			if lower == 127 {
				return Err("Unsupported reserved length type".into());
			}

			let n = lower as usize;
			if n > USIZE_OCTETS {
				return Err("Too many length octets".into());
			}

			let mut buf = [0u8; USIZE_OCTETS];

			let data = c.next(take_exact(n))?;
			buf[(USIZE_OCTETS - n)..].copy_from_slice(&data);

			let val = usize::from_be_bytes(buf);
			Ok(Self::Long(val))
		}
	}) });

	/// Serializes the length using them minimal possible representation.
	fn serialize(len: Option<usize>, out: &mut Vec<u8>) {
		let len = match len {
			Some(n) => n,
			None => { out.push(0x80); return; }
		};

		if len <= 127 {
			out.push(len as u8);
			return;
		}

		let buf = len.to_be_bytes();
		let i = buf.iter().position(|v| *v != 0).unwrap_or(buf.len());
		let nbytes = buf.len() - i;

		out.push(0x80 | (nbytes as u8));
		out.extend_from_slice(&buf[i..]);
	}
}

#[derive(Debug, Clone)]
pub struct Element {
	pub ident: Identifier,
	pub len: Length,
	pub data: Bytes,
	pub outer: Bytes
}

impl Element {
	parser!(pub parse<Self> => {
		map(slice_with(seq!(c => {
			let ident = c.next(Identifier::parse)?;
			let len = c.next(Length::parse)?;

			// TODO: Check that only certain tag numbers are allowed to be
			// constructed.

			// TODO: Length must be definite if all data is immediately available
			// or if the value is a primitive.

			let data = match len {
				Length::Short(n) => {
					c.next(take_exact(n as usize))?
				},
				Length::Long(n) => {
					c.next(take_exact(n))?
				},
				Length::Indefinite => {
					return Err("Indefinite parsing not supported".into());
				}
			};

			// let value =
			// 	if ident.constructed {
			// 		let (inner, _) = complete(many(Self::parse))(data)?;
			// 		ElementValue::Constructed(inner)
			// 	} else {
			// 		ElementValue::Primitive(data)
			// 	};

			Ok((ident, len, data))
//			Ok(Self { ident, len, data })
		})), |((ident, len, data), outer)| Self { ident, len, data, outer } )
	});
}

pub fn parse_ber(data: Bytes) -> Result<()> {
	let (v, rest) = Element::parse(data)?;
	println!("{:?}", v);
	println!("Remaining: {}", rest.len());

	Ok(())
}

macro_rules! some_or_else {
	($e:expr) => {
		match $e {
			Some(v) => v,
			None => { return Ok(None); }
		}
	};
}


use std::collections::HashMap;
use chrono::DateTime;

#[derive(Debug)]
enum DERReaderBuffer {
	Empty,
	Unparsed(Bytes),
	Single(Element),
	Parsed(HashMap<Tag, Element>)
}

// TODO: It is important to ensure that we read all of the input till the end
// of each reader.

fn is_printable_string_char(c: char) -> bool {
	match c {
		'A' ..= 'Z' | 'a' ..= 'z' | '0' ..= '9' | ' ' | '\'' | '(' | ')'
		| '+' | ',' | '-' | '.' | '/' | ':' | '=' | '?' => true,
		_ => false
	}
}


pub struct DERReader {
	remaining: DERReaderBuffer,
	// Ranges of data inside of Elements in the order they were parsed.
	pub slices: Vec<Bytes>,
	implicit_tag: Option<Tag>,
	// Number of elements that have been taken out of remaining.
	elements_read: usize
}

impl DERReader {
	pub fn new(input: Bytes) -> Self {
		Self::from_buffer(DERReaderBuffer::Unparsed(input))
	}

	fn from_buffer(buf: DERReaderBuffer) -> Self {
		Self {
			remaining: buf,
			slices: vec![],
			implicit_tag: None,
			elements_read: 0
		}
	}

	fn read_any_element(&mut self) -> Result<Element> {
		if self.implicit_tag.is_some() {
			return Err("Any not supported with an implicit tag".into());
		}

		let el: Result<Element> = match &mut self.remaining {
			DERReaderBuffer::Empty => {
				Err("No remaining data in reader".into())
			},
			DERReaderBuffer::Unparsed(buffer) => {
				let (el, rest) = Element::parse(buffer.clone())?;
				self.remaining = DERReaderBuffer::Unparsed(rest);
				self.elements_read += 1;
				Ok(el)
			},
			DERReaderBuffer::Single(el) => {
				let out = el.clone();
				self.remaining = DERReaderBuffer::Empty;
				self.elements_read += 1;
				Ok(out)
			},
			DERReaderBuffer::Parsed(map) => {
				Err("Can't read ANY in set.".into())
			}
		};

		let el = el?;

		self.slices.push(el.outer.clone());
		Ok(el)
	}

	fn read_element(&mut self, class: TagClass, number: usize,
					constructed: bool) -> Result<Bytes> {
		let tag = self.implicit_tag.take().unwrap_or(Tag { class, number });
		
		let res: Result<(Bytes, Bytes)> = match &mut self.remaining {
			DERReaderBuffer::Empty => {
				Err("No remaining data in reader".into())
			},
			DERReaderBuffer::Unparsed(buffer) => {
				let (el, rest) = Element::parse(buffer.clone())?;

				if tag == el.ident.tag {
					if constructed != el.ident.constructed {
						return Err("Mismatch in P/C type".into());
					}

					self.remaining = DERReaderBuffer::Unparsed(rest);
					self.elements_read += 1;
					Ok((el.data, el.outer))
				} else {
					// println!("wRONG TAG: {:?} {} {:?} {}", el.ident.tag, el.ident.constructed, tag, constructed);
					Err("Wrong tag".into())
				}
			},
			DERReaderBuffer::Single(el) => {
				if tag == el.ident.tag {
					if constructed != el.ident.constructed {
						return Err("Mismatch in P/C type".into());
					}

					// TODO: Get rid of this clone.
					let out = el.data.clone();
					let outer = el.outer.clone();
					self.remaining = DERReaderBuffer::Empty;
					self.elements_read += 1;
					Ok((out, outer))
				} else {
					Err("Wrong tag".into())
				}
			},
			DERReaderBuffer::Parsed(map) => {
				if let Some(el) = map.get(&tag) {
					if constructed != el.ident.constructed {
						return Err("Mismatch in P/C type".into());
					}

					let out = el.data.clone();
					let outer = el.outer.clone();
					map.remove(&tag);
					self.elements_read += 1;
					Ok((out, outer))

				} else {
					Err("Missing tag in elements".into())
				}
			}
		};
		let (el, outer) = res?;

		self.slices.push(outer);
		Ok(el)
	}

	/// NOTE: If this internally reads an explicitly tagged element, then
	/// we assume that no element immediately after it has the same tag
	pub fn read_option<T, F: FnMut(&mut DERReader) -> Result<T>>(
		&mut self, mut f: F
	) -> Result<Option<T>> {
		let initial_num = self.elements_read;
		let res = f(self);
		match res {
			Ok(v) => Ok(Some(v)),
			Err(e) => {
				// NOTE: We assume that there is no other code in the inner
				// parser that could error out before the read_element call.
				if self.elements_read == initial_num {
					Ok(None)
				} else {
					Err(e)
				}
			}
		}
	}

	// TODO: If using DERWriteable going to end up a cyclic reference?
	pub fn read_with_default<T: DERWriteable, F: FnMut(&mut DERReader) -> Result<T>>(
		&mut self, mut f: F, default_value: T) -> Result<T> {
		let value = self.read_option(f)?;
		match value {
			Some(v) => {
				if der_eq(&v, &default_value) {
					return Err("DERReader saw default value encoded.".into());
				}

				Ok(v)
			},
			None => Ok(default_value)
		}
	}


	pub fn read_implicitly<T, F: FnMut(&mut DERReader) -> T>(
		&mut self, tag: Tag, mut f: F) -> T {
		if self.implicit_tag.is_none() {
			self.implicit_tag = Some(tag);
		}

		let ret = f(self);

		// This may be needed if the implicit tag was unused due to something
		// like an optional field.
		self.implicit_tag = None;
		
		ret
	}

	pub fn read_explicitly<T, F: FnMut(&mut DERReader) -> Result<T>>(
		&mut self, tag: Tag, mut f: F) -> Result<T> {

		let data = self.read_element(tag.class, tag.number, true)?;
		
		let mut reader = DERReader::new(data);
		let v = f(&mut reader)?;

		// TODO: Validate that we are always checking for this.
		if !reader.is_finished() { // reader.remaining.len() > 0 {
			return Err("Explicitly typed object contains extra data".into());
		}

		Ok(v)
	}

	// The calling function should try to read each inner type using read_option
	// and pick the first that suceeds
	pub fn read_choice<T, F: FnMut(&mut DERReader) -> Result<T>>(
		&mut self, mut f: F) -> Result<T> {
		if let Some(t) = self.implicit_tag.take() {
			self.read_explicitly(t, f)
		} else {
			f(self)
		}
	}

	pub fn read_any(&mut self) -> Result<Element> {
		// TODO: Should read recursively to validate it.
		let el = self.read_any_element()?;
		// println!("ANY: {:?}", el);
		Ok(el)
	}

	pub fn read_bool(&mut self) -> Result<bool> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_BOOLEAN, false)?;

		if data.len() != 1 {
			Err("Data wrong size".into())
		} else if data[0] == 0x00 {
			Ok(false)
		} else if data[0] == 0xff {
			Ok(true)
		} else {
			Err("Invalid boolean value".into())
		}
	}

	pub fn read_int(&mut self) -> Result<BigInt> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_INTEGER, false)?;

		let n = BigInt::from_be_bytes(&data);
		
		// Always at least one byte.
		if common::ceil_div(std::cmp::max(n.nbits(), 8), 8) != data.len() {
			println!("{:?}", data);
			println!("{:?} {} {}", n, n.nbits(), data.len());
			return Err("Integer not minimal length".into());
		}

		Ok(n)
	}

	pub fn read_isize(&mut self) -> Result<isize> {
		let val = self.read_int()?;
		val.to_isize()
	}

	pub fn read_bitstring(&mut self) -> Result<BitString> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_BIT_STRING, false)?;
		if data.len() < 1 {
			return Err("Bitstring too short".into());
		}
		let nunused = data[0];
		if data.len() == 1 {
			if nunused != 0 {
				return Err("Empty data but not 0 unused".into());
			}
			return Ok(BitString { data: BitVector::new() })
		}
		if nunused >= 8 {
			return Err("Nunused too long".into());
		}
		let data = BitVector::from(&data[1..], 8*(data.len() - 1)
									- (nunused as usize));
		Ok(BitString { data })
	}

	pub fn read_octetstring(&mut self) -> Result<OctetString> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_OCTET_STRING, false)?;
		Ok(OctetString(data.into()))
	}

	pub fn read_null(&mut self) -> Result<()> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_NULL, false)?;
		if data.len() != 0 {
			return Err("Expected no data in NULL".into());
		}

		Ok(())
	}

	pub fn read_oid(&mut self) -> Result<ObjectIdentifier> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_OBJECT_IDENTIFIER, false)?;
		let (items, _) = complete(seq!(c => {
			// NOTE: Every absolute OID will have at least 2 components.
			let first = c.next(be_u8)?;
			let val1 = (first / 40) as usize;
			let val2 = (first % 40) as usize;
			if val1 > 2 {
				return Err("First component can only be 0,1, or 2".into());
			}

			let mut arr = vec![ val1, val2 ];
			let extra = c.next(many(parse_varint_msb_be))?;
			arr.extend_from_slice(&extra);
			Ok(arr)
		}))(data)?;

		Ok(ObjectIdentifier::from_vec(items))
	}

	/// NOTE: We don't currently support enums larger than the machine size.
	pub fn read_enumerated(&mut self) -> Result<isize> {
		self.read_implicitly(
			Tag { class: TagClass::Universal, number: TAG_NUMBER_ENUMERATED },
			|r| r.read_isize())
	}

	pub fn read_utf8string(&mut self) -> Result<UTF8String> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_UTF8STRING, false)?;
		Ok(UTF8String::from(data)?)
	}

	// TODO: When extensible, we should support storing them unparsed in the
	// struct so can be inspected.
	pub fn read_sequence<T, F: Fn(&mut DERReader) -> Result<T>>(
		&mut self, extensible: bool, f: F) -> Result<T> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_SEQUENCE, true)?;

		let mut reader = Self::new(data);
		let out = f(&mut reader)?;

		if !extensible {
			reader.finished()?;
		}

		self.slices.append(&mut reader.slices);

		Ok(out)
	}

	pub fn read_sequence_of<T: DERReadable>(&mut self)
	-> Result<Vec<T>> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_SEQUENCE, true)?;
		
		let mut items = vec![];
		let mut reader = Self::new(data);
		while !reader.is_finished() { // reader.remaining.len() > 0 {
			items.push(T::read_der(&mut reader)?);
		}
		self.slices.append(&mut reader.slices);

		Ok(items)
	}

	pub fn read_set<T, F: Fn(&mut DERReader) -> Result<T>>(
		&mut self, extensible: bool, f: F) -> Result<T> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_SET, true)?;
		let (elements, _) = complete(many(Element::parse))(data)?;

		// Check that they are sorted by raw serialized data.
		for i in 1..elements.len() {
			if elements[i - 1].ident.tag >= elements[i].ident.tag {
				return Err(
					"Set elements not sorted or have duplicate tags".into());
			}
		}

		let mut map = HashMap::new();
		for e in elements.into_iter() {
			let t = e.ident.tag.clone();
			map.insert(t, e);
		}

		let mut reader = DERReader::from_buffer(DERReaderBuffer::Parsed(map));
		let out = f(&mut reader)?;

		if !extensible {
			reader.finished()?;
		}

		self.slices.append(&mut reader.slices);

		Ok(out)
	}

	pub fn read_set_of<T: DERReadable>(&mut self) -> Result<Vec<T>> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_SET, true)?;
		let (elements, _) = complete(many(slice_with(Element::parse)))(data)?;

		// Check that they are sorted by raw serialized data.
		for i in 1..elements.len() {
			if elements[i - 1].1 > elements[i].1 {
				return Err("SetOf elements not sorted".into());
			}
		}

		let mut out = vec![];
		out.reserve(elements.len());

		// TODO: In this case, they will most likely be identical.
		for (e, _) in elements.into_iter() {
			let mut reader = DERReader::from_buffer(DERReaderBuffer::Single(e));
			let item = T::read_der(&mut reader)?;
			reader.finished()?;
			out.push(item);
			self.slices.append(&mut reader.slices);
		}
		
		Ok(out)
	}

	pub fn read_printable_string(&mut self) -> Result<AsciiString> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_PRINTABLE_STRING, false)?;
		for b in &data {
			if !is_printable_string_char(*b as char) {
				return Err("Invalid printable string character".into());
			}
		}

		// NOTE: ascii-ness is guranteed by is_printable_string_char
		Ok(unsafe { AsciiString::from_ascii_unchecked(data) })
	}

	pub fn read_t61_string(&mut self) -> Result<String> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_T61STRING, false)?;
		super::t61::t61_parse(&data)
	}

	pub fn read_ia5string(&mut self) -> Result<AsciiString> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_IA5STRING, false)?;
		AsciiString::from_bytes(data)
	}

	pub fn read_utctime(&mut self) -> Result<UTCTime> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_UTCTIME, false)?;
		let s = AsciiString::from_bytes(data)?;
		UTCTime::from_str(s.as_ref())
	}

	pub fn read_generalized_time(&mut self) -> Result<GeneralizedTime> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_GENERALIZEDTIME, false)?;
		let s = AsciiString::from_bytes(data)?;
		GeneralizedTime::from_str(s.as_ref())
	}

	// UCS-4
	pub fn read_universal_string(&mut self) -> Result<String> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_UNIVERSALSTRING, false)?;
		let (chars, _) = complete(many(be_u32))(data)?;
		
		let mut s = String::new();
		s.reserve(chars.len());
		for c in chars.into_iter() {
			s.push(char::try_from(c)?);
		}

		Ok(s)
	}

	/// UTF-16 Big Endian encoded string
	pub fn read_bmp_string(&mut self) -> Result<BMPString> {
		let data = self.read_element(
			TagClass::Universal, TAG_NUMBER_BMPSTRING, false)?;
		let (codes, _) = complete(many(be_u16))(data)?;
		Ok(BMPString {
			data: String::from_utf16(&codes)?
		})
	}

	fn is_finished(&self) -> bool {
		match &self.remaining {
			DERReaderBuffer::Empty => true,
			DERReaderBuffer::Single(_) => false,
			DERReaderBuffer::Unparsed(buf) => buf.len() == 0,
			DERReaderBuffer::Parsed(map) => map.len() == 0
		}
	}

	/// Call at the end of parsing to ensure that all of the input was consumed.
	pub fn finished(&self) -> Result<()> {
		if !self.is_finished() {
			return Err(format!("Not finished {:?}", self.remaining).into())
		}
		Ok(())
	}
}




/// NOTE: This object does not guranteed the DER rule that fields of a
/// SET/SEQUENCE with value equal to the default value are not included.
/// It is up to the caller to ensure that the appropriate method is not called
/// if this does happen.
pub struct DERWriter<'a> {
	out: &'a mut Vec<u8>,
	
	/// Contains the indices of the start of each element.
	indices: Vec<usize>,

	implicit_tag: Option<Tag>
}

// NOTE: Unless overriden, tag'ed types use the context-specific class
// NOTE: Writing a CHOICE that has an outer implicit tag effectively ends up
// being an explicit tag
impl<'a> DERWriter<'a> {
	pub fn new(out: &'a mut Vec<u8>) -> Self {
		Self { out, indices: vec![], implicit_tag: None }
	}

	fn into_indices(self) -> Vec<usize> {
		self.indices
	}

	fn write_tag(&mut self, class: TagClass, number: usize, constructed: bool) {
		let tag = self.implicit_tag.take().unwrap_or(Tag { class, number });
		let ident = Identifier { tag, constructed };
		self.indices.push(self.out.len());
		ident.serialize(self.out);
	}

	fn write_length(&mut self, len: usize) {
		Length::serialize(Some(len), self.out);
	}

	// TODO: It is important to ensure that write_implicitly and write_explicitly are only only used to write a single element in the callback.

	pub fn write_implicitly<F: FnMut(&mut DERWriter)>(
		&mut self, tag: Tag, mut f: F) {
		if self.implicit_tag.is_none() {
			self.implicit_tag = Some(tag);
		}

		f(self);

		// This may be needed if the implicit tag was unused due to something
		// like an optional field.
		self.implicit_tag = None;
	}

	pub fn write_explicitly<F: FnMut(&mut DERWriter)>(
		&mut self, tag: Tag, mut f: F) {
		let mut data = vec![];
		{
			let mut writer = DERWriter::new(&mut data);
			f(&mut writer);
		}

		if data.len() == 0 {
			return;
		}

		self.write_tag(tag.class, tag.number, true);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	pub fn write_choice<F: FnMut(&mut DERWriter)>(&mut self, mut f: F) {
		// When an implicit tag is specified, it turns into an explicit tag.
		// TODO: This may only be inside of sets?
		if let Some(t) = self.implicit_tag.take() {
			self.write_explicitly(t, f);
		} else {
			f(self);
		}
	}

	pub fn write_bool(&mut self, value: bool) {
		self.write_tag(TagClass::Universal, TAG_NUMBER_BOOLEAN, false);
		self.write_length(1);
		self.out.push(if value { 0xff } else { 0 });
	}

	/// Big endian two's complement encoded in one or more octets in as
	/// few octets as possible.
	pub fn write_int(&mut self, value: &BigInt) {
		let mut data = value.to_be_bytes();
		if data.len() == 0 {
			data.push(0);
		}

		self.write_tag(TagClass::Universal, TAG_NUMBER_INTEGER, false);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	pub fn write_isize(&mut self, value: isize) {
		self.write_int(&BigInt::from_isize(value))
	}

	/// In the primitive form, the first octet of the content is a u8 indicating
	/// the number of unused bits in the final. Bits are ordered from
	/// MSB to LSB.
	pub fn write_bitstring(&mut self, bits: &BitString) {
		let octets = bits.data.as_ref();
		let nunused = 8*octets.len() - bits.data.len();

		self.write_tag(TagClass::Universal, TAG_NUMBER_BIT_STRING, false);
		self.write_length(octets.len() + 1);
		self.out.push(nunused as u8);
		self.out.extend_from_slice(octets);
	}

	pub fn write_octetstring(&mut self, octets: &[u8]) {
		self.write_tag(TagClass::Universal, TAG_NUMBER_OCTET_STRING, false);
		self.write_length(octets.len());
		self.out.extend_from_slice(octets);
	}

	pub fn write_null(&mut self) {
		self.write_tag(TagClass::Universal, TAG_NUMBER_NULL, false);
		self.write_length(0);
	}

	pub fn write_oid(&mut self, oid: &ObjectIdentifier) {
		let components = oid.as_ref();
		assert!(components.len() >= 2);
		assert!(components[0] <= 2);
		assert!(components[1] < 40);

		let mut data = vec![];
		data.push((40*components[0] + components[1]) as u8);
		for c in &components[2..] {
			serialize_varint_msb_be(*c, &mut data);
		}

		self.write_tag(
			TagClass::Universal, TAG_NUMBER_OBJECT_IDENTIFIER, false);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	// TODO: Should only allow a usize?
	pub fn write_enumerated(&mut self, v: isize) {
		self.write_implicitly(
			Tag { class: TagClass::Universal, number: TAG_NUMBER_ENUMERATED },
			move |w| w.write_isize(v));
	}

	pub fn write_sequence<F: Fn(&mut DERWriter)>(&mut self, f: F) {
		let mut data = vec![];
		{
			let mut writer = DERWriter::new(&mut data);
			f(&mut writer);
		}

		self.write_tag(TagClass::Universal, TAG_NUMBER_SEQUENCE, true);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	pub fn write_sequence_of<T: DERWriteable>(&mut self, items: &[T]) {
		let data = {
			let mut data = vec![];
			let mut writer = DERWriter::new(&mut data);
			for i in items {
				i.write_der(&mut writer);
			}

			data
		};

		// TODO: Dedup this and the set_of one with the base write_sequence impl.
		self.write_tag(TagClass::Universal, TAG_NUMBER_SEQUENCE, true);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	pub fn write_set<F: Fn(&mut DERWriter)>(&mut self, f: F) {
		let slices: Vec<Bytes> = {
			let mut data = vec![];
			let mut writer = DERWriter::new(&mut data);
			f(&mut writer);

			let mut indices = writer.into_indices();
			indices.push(data.len());

			let buf = Bytes::from(data);
			let mut out = vec![];
			for i in 0..(indices.len() - 1) {
				out.push(buf.slice(indices[i]..indices[i + 1]));
			}

			out
		};

		let mut elements = slices.into_iter().map(|data| {
			let (ident, _) = Identifier::parse(data.clone()).unwrap();
			(ident.tag, data)
		}).collect::<Vec<_>>();

		elements.sort_by(|a, b| {
			match a.0.cmp(&b.0) {
				// It is invalid to have a SET without distinct tags for each
				// field.
				std::cmp::Ordering::Equal => panic!("Set items with same tag"),
				v @ _ => v
			}
		});

		let len = elements.iter().fold(0, |v, (i, data)| v + data.len());

		self.write_tag(TagClass::Universal, TAG_NUMBER_SET, true);
		self.write_length(len);
		self.out.reserve(len);
		for (_, data) in elements {
			self.out.extend_from_slice(&data);
		}
	}
	
	// TODO: Do same thing as 
	pub fn write_set_of<T: DERWriteable>(&mut self, items: &[T]) {
		let mut elements = vec![];
		elements.reserve(items.len());

		for i in items {
			let mut data = vec![];
			{
				let mut writer = DERWriter::new(&mut data);
				i.write_der(&mut writer);
			}
			elements.push(data)
		}

		elements.sort();

		let len = elements.iter().fold(0, |v, data| v + data.len());

		self.write_tag(TagClass::Universal, TAG_NUMBER_SET, true);
		self.write_length(len);
		self.out.reserve(len);
		for data in elements {
			self.out.extend_from_slice(&data);
		}
	}

	pub fn write_printable_string(&mut self, s: &AsciiString) {
		for c in s.as_ref().chars() {
			if !is_printable_string_char(c) {
				panic!("Writing string with non printable chars!");
			}
		}

		self.write_tag(TagClass::Universal, TAG_NUMBER_PRINTABLE_STRING, false);
		self.write_length(s.data.len());
		self.out.extend_from_slice(&s.data);
	}

	pub fn write_t61_string(&mut self, s: &str) {
		self.write_tag(TagClass::Universal, TAG_NUMBER_T61STRING, false);
		let data = super::t61::t61_serialize(s).unwrap();
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	pub fn write_ia5string(&mut self, s: &AsciiString) {
		self.write_tag(TagClass::Universal, TAG_NUMBER_IA5STRING, false);
		self.write_length(s.data.len());
		self.out.extend_from_slice(&s.data);
	}

	pub fn write_utctime(&mut self, time: &UTCTime) {
		self.write_tag(TagClass::Universal, TAG_NUMBER_UTCTIME, false);
		let s = time.to_string();
		self.write_length(s.as_bytes().len());
		self.out.extend_from_slice(s.as_bytes());
	}

	pub fn write_generalized_time(&mut self, time: &GeneralizedTime) {
		self.write_tag(
			TagClass::Universal, TAG_NUMBER_GENERALIZEDTIME, false);
		let s = time.to_string();
		self.write_length(s.as_bytes().len());
		self.out.extend_from_slice(s.as_bytes());
	}

	pub fn write_universal_string(&mut self, s: &str) {
		let mut data = vec![];
		data.reserve(4*s.len()); // NOTE: This is approximate
		for c in s.chars() {
			data.extend_from_slice(&(c as u32).to_be_bytes());
		}

		self.write_tag(
			TagClass::Universal, TAG_NUMBER_UNIVERSALSTRING, false);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	pub fn write_bmp_string(&mut self, s: &str) {
		let mut data = vec![];
		for v in s.encode_utf16() {
			data.extend_from_slice(&v.to_be_bytes());
		}

		self.write_tag(
			TagClass::Universal, TAG_NUMBER_BMPSTRING, false);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	// DER Rules:
	// - For 'SET OF' the components are sorted based on the binary encoding value.
	// - If a default value is present, we should never encode any value equal to it.	

	// - For SET: "The canonical order for tags is based on the outermost tag of each type and is defined as follows:"

}

// pub struct DERWriterIter {
// 	items: Vec<Vec<u8>>
// }

// impl DERWriterIter {
// 	pub fn new() -> Self {
// 		Self { items: vec![] }
// 	}

// 	pub fn next(&mut self) -> DERWriter {
// 		self.items.push(vec![]);
// 		// TODO: This writer should only be allowed to write a single tag.
// 		DERWriter::new(self.items.last_mut().unwrap())
// 	} 

// 	pub fn into_inner(mut self) -> Vec<Vec<u8>> {
// 		self.items
// 	}
// }


pub trait DERWriteable {
	fn write_der(&self, writer: &mut DERWriter);

	fn to_der(&self) -> Vec<u8> {
		let mut buf = vec![];
		{
			let mut writer = DERWriter::new(&mut buf);
			self.write_der(&mut writer);
		}
		buf
	}
}



impl DERWriteable for bool {
	fn write_der(&self, writer: &mut DERWriter) { writer.write_bool(*self); }
}
impl DERWriteable for BigInt {
	fn write_der(&self, writer: &mut DERWriter) { writer.write_int(self); }
}
impl DERWriteable for BitString {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_bitstring(self);
	}
}
impl DERWriteable for OctetString {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_octetstring(self.as_ref());
	}
}
impl DERWriteable for Null {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_null();
	}
}
impl<T: DERWriteable + Debug> DERWriteable for SequenceOf<T> {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_sequence_of(&self.items);
	}
}
impl<T: DERWriteable + Debug> DERWriteable for SetOf<T> {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_set_of(&self.items);
	}
}
impl DERWriteable for ObjectIdentifier {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_oid(self);
	}
}
impl DERWriteable for UTF8String {
	fn write_der(&self, writer: &mut DERWriter) {
		unimplemented!("TODO: UTF8String");
	}
}
impl DERWriteable for NumericString {
	fn write_der(&self, writer: &mut DERWriter) {
		unimplemented!("TODO: NumericString");
	}
}


impl DERWriteable for PrintableString {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_printable_string(&self.0);
	}
}
impl DERWriteable for TeletexString {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_t61_string(&self.data)
	}
}
// const TAG_NUMBER_VIDEOTEXSTRING: usize = 21;

impl DERWriteable for IA5String {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_ia5string(&self.data);
	}
}


impl DERWriteable for UTCTime {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_utctime(self);
	}
}

impl DERWriteable for GeneralizedTime {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_generalized_time(self);
	}
}

// const TAG_NUMBER_GRAPHICSTRING: usize = 25;

impl DERWriteable for VisibleString {
	fn write_der(&self, writer: &mut DERWriter) {
		unimplemented!("TODO: VisibleString");
	}
}
// const TAG_NUMBER_GENERALSTRING: usize = 27;

impl DERWriteable for UniversalString {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_universal_string(&self.data);
	}
}
// const TAG_NUMBER_CHARACTER_STRING: usize = 29;

impl DERWriteable for BMPString {
	fn write_der(&self, writer: &mut DERWriter) {
		writer.write_bmp_string(&self.data);
	}
}
// const TAG_NUMBER_DATE: usize = 31;
// const TAG_NUMBER_TIME_OF_DAY: usize = 32;
// const TAG_NUMBER_DATE_TIME: usize = 33;


pub trait DERReadable: Sized {
	fn read_der(reader: &mut DERReader) -> Result<Self>;

	fn from_der(data: Bytes) -> Result<Self> {
		let mut reader = DERReader::new(data);
		let out = Self::read_der(&mut reader)?;
		reader.finished()?;
		Ok(out)
	}
}

impl DERReadable for bool {
	fn read_der(reader: &mut DERReader) -> Result<Self> { reader.read_bool() }
}
impl DERReadable for BigInt {
	fn read_der(reader: &mut DERReader) -> Result<Self> { reader.read_int() }
}
impl DERReadable for BitString {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		reader.read_bitstring()
	}
}
impl DERReadable for OctetString {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		reader.read_octetstring()
	}
}
impl DERReadable for Null {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		reader.read_null()?;
		Ok(Null::new())
	}
}
impl DERReadable for ObjectIdentifier {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		reader.read_oid()
	}
}
impl DERReadable for UTF8String {
	fn read_der(r: &mut DERReader) -> Result<Self> { r.read_utf8string() }
}

impl<T: DERReadable + Debug> DERReadable for SequenceOf<T> {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		let items = reader.read_sequence_of()?;
		Ok(Self { items })
	}
}
impl<T: DERReadable + Debug> DERReadable for SetOf<T> {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		let items = reader.read_set_of()?;
		Ok(Self { items })
	}
}
impl DERReadable for NumericString {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		// TODO
		unimplemented!("TODO: NumericString");
		Ok(Self {})
	}
}
impl DERReadable for PrintableString {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		Ok(Self(reader.read_printable_string()?))
	}
}
impl DERReadable for TeletexString {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		Ok(Self { data: reader.read_t61_string()? })
	}
}
impl DERReadable for IA5String {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		Ok(Self { data: reader.read_ia5string()? })
	}
}
impl DERReadable for UTCTime {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		reader.read_utctime()
	}
}
impl DERReadable for GeneralizedTime {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		reader.read_generalized_time()
	}
}
impl DERReadable for VisibleString {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		// TODO
		unimplemented!("TODO: VisibleString");
		Ok(Self {})
	}
}
impl DERReadable for UniversalString {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		Ok(Self { data: reader.read_universal_string()? })
	}
}
impl DERReadable for BMPString {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		reader.read_bmp_string()
	}
}



// TODO: We must always have fully parsed Anys to ensure that the entire DER
// is valid? (but then what about unknown extensions?)
#[derive(Clone)]
pub struct Any {
	element: Element
}

impl Any {
	pub fn from(data: Bytes) -> Result<Self> {
		let (element, _) = complete(Element::parse)(data)?;
		Ok(Self { element })
	}

	pub fn parse_as<T: DERReadable>(&self) -> Result<T> {
		let mut reader = DERReader::from_buffer(
			DERReaderBuffer::Single(self.element.clone()));
		let out = T::read_der(&mut reader)?;
		reader.finished()?;
		Ok(out)
	}

	// pub const fn from_static(v: &'static (dyn DERWriteable + Send + Sync)) -> Self {
	// 	Self::Reference(v)
	// }
}

//impl<T: DERWriteable> std::convert::From<T> for Any {
//	fn from(value: T) -> Self {
//		Self::from(value.to_der().into())
//	}
//}

impl std::fmt::Debug for Any {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: At least print the element form.
		write!(f, "Any({:?})", self.element)
    }
}

impl DERWriteable for Any {
	fn write_der(&self, writer: &mut DERWriter) {
		// TODO: Have a more elegant way of doing this.
		writer.write_tag(
			self.element.ident.tag.class, self.element.ident.tag.number, self.element.ident.constructed);
		// TODO: Does not preserve the original form?
		writer.write_length(self.element.data.len());
		writer.out.extend_from_slice(&self.element.data);
	}
}

impl DERReadable for Any {
	fn read_der(reader: &mut DERReader) -> Result<Self> {
		Ok(Self { element: reader.read_any()? })
	}
}

// NOTE: This isn't actually used in any generated code as it doesn't handle
// prefixes and other encoding constraings. It should only be used by PartialEq.
impl<T: DERWriteable> DERWriteable for Option<T> {
	fn write_der(&self, writer: &mut DERWriter) {
		if let Some(v) = self {
			v.write_der(writer);
		}
	}
}

pub fn der_eq<T: DERWriteable, Y: DERWriteable>(a: &T, b: &Y) -> bool {
	let aa = a.to_der();
	let bb = b.to_der();
	aa == bb
}


impl<T: DERWriteable> PartialEq<T> for Any {
	fn eq(&self, other: &T) -> bool {
		let a = self.to_der();
		let b = other.to_der();
		a == b
	}
}

#[macro_export]
macro_rules! asn_any {
	($e:expr) => {{
		Any::from(::bytes::Bytes::from($e.to_der())).unwrap()
	}};
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn length_test() {
//		let lens = vec![
//			Length::Indefinite, Length::Short(0), Length::Short(12),
//			Length::Short(127), Length::Long(52), Length::Long(123456)
//		];

		let lens = vec![0, 1, 2, 3, 12, 52, 127, 123456];

		for l in lens {
			let mut enc = vec![];
			Length::serialize(Some(l), &mut enc);
			let (dec, rest) = Length::parse(enc.into()).unwrap();
			assert_eq!(rest.len(), 0);

			let out = match dec {
				Length::Long(l) => l,
				Length::Short(l) => l as usize,
				_ => panic!("Did not expect an indefinite length")
			};
			assert_eq!(out, l);
		}
	}

	#[test]
	fn oid_test() {
		let id = ObjectIdentifier::from(&[1,2,840,113549,1,1,5]);
		let mut out = id.to_der();
		println!("{:?}", out);
		let mut id2 = ObjectIdentifier::from_der(out.into()).unwrap();
		assert_eq!(id, id2);
	}
}