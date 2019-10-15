use parsing::*;
use parsing::binary::*;
use parsing::ascii::AsciiString;
use bytes::Bytes;
use common::errors::*;
use super::builtin::*;

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

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub enum TagClass {
	Universal = 0,
	Application = 1,
	ContextSpecific = 2,
	Private = 3
}
impl TagClass {
	fn from(v: u8) -> Self {
		match v {
			0 => TagClass::Universal,
			1 => TagClass::Application,
			2 => TagClass::ContextSpecific,
			3 => TagClass::Private,
			_ => panic!("Value larger than 2 bits")
		}
	}
}

/// NOTE: Tags have a canonical ordering of Universal, Application, Context,
/// Private, and then in each class it is in order of ascending number.
#[derive(Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct Tag {
	pub class: TagClass,
	pub number: usize
}

#[derive(Debug)]
struct Identifier {
	tag: Tag,
	// If not constructed, then it is a primitive.
	constructed: bool
}

impl Identifier {
	parser!(parse<Self> => { seq!(c => {
		let first = c.next(be_u8)?;
		let class = TagClass::from((first >> 6) & 0b11);
		let constructed = ((first >> 5) & 0b1) == 1;
		let mut number = (first & 0b11111) as usize;

		// In this case, the tag takes 2+ octets.
		// Stored as big-endian chunks of 7 lower bits of each next octet.
		if number == 31 {
			number = 0;
			
			let mut finished = false;
			for i in 0..(MAX_TAG_NUMBER_BITS / 7) {
				let octet = c.next(be_u8)?;
				let num_part = octet & 0x7f; // Lower 7 bits
				finished |= (octet >> 7) == 1; // Upper 1 bit

				number = number << 7;
				number |= num_part as usize;

				if finished {
					// The tag number should be encoded minimally meaning the
					// last octet must have a non-zero tag number part.
					if num_part == 0 {
						return Err("Last tag octet contains zero".into());
					}
					break;
				}
			}

			if !finished {
				return Err("Tag number overflow integer range".into());
			}

			// The 2+ octet form should only be used for numbers >= 31
			if number <= 30 {
				return Err("Should have used single octet".into());
			}
		}

		Ok(Self { tag: Tag { class, number }, constructed })
	}) });

	fn serialize(&self, out: &mut Vec<u8>) {
		let first = ((self.tag.class as u8) << 6)
			| (if self.constructed { 1 } else { 0 })
			| (if self.tag.number <= 30 { self.tag.number as u8 } else { 31 });
		out.push(first);

		// TODO: Deduplicate the varint code with the other crates.
		if self.tag.number >= 31 {
			let mut num = self.tag.number;
			let mut buf = [0u8; std::mem::size_of::<usize>()];
			let mut i = buf.len() - 1;
			loop {
				let b = (num & 0x7f) as u8;
				num >> 7;

				buf[i] = b;
				if num == 0 {
					break;
				} else {
					buf[i] |= 0x80;
				}

				i -= 1;
			}

			out.extend_from_slice(&buf[i..]);
		}
	}
}

#[derive(Debug)]
enum Length {
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

// #[derive(Debug)]
// enum ElementValue {
// 	Primitive(Bytes),
// 	Constructed(Vec<Element>)
// }

#[derive(Debug)]
struct Element {
	ident: Identifier,
	len: Length,
	data: Bytes
	// value: ElementValue
}

impl Element {
	parser!(parse<Self> => {
		seq!(c => {
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

			Ok(Self { ident, len, data })
		})
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


const TAG_NUMBER_BOOLEAN: usize = 1;
const TAG_NUMBER_INTEGER: usize = 2;
const TAG_NUMBER_BIT_STRING: usize = 3;
const TAG_NUMBER_OCTET_STRING: usize = 4;
const TAG_NUMBER_NULL: usize = 5;
const TAG_NUMBER_OBJECT_IDENTIFIER: usize = 6;
const TAG_NUMBER_OBJECT_DESCRIPTOR: usize = 7;
const TAG_NUMBER_EXTERNAL: usize = 8;
const TAG_NUMBER_REAL: usize = 9;
const TAG_NUMBER_ENUMERATED: usize = 10;
const TAG_NUMBER_EMBEDDED_PDV: usize = 11;
const TAG_NUMBER_UTF8STRING: usize = 12;
const TAG_NUMBER_RELATIVE_OID: usize = 13;
const TAG_NUMBER_TIME: usize = 14;
const TAG_NUMBER_SEQUENCE: usize = 16;
const TAG_NUMBER_SET: usize = 17;
const TAG_NUMBER_NUMERIC_STRING: usize = 18;
const TAG_NUMBER_PRINTABLE_STRING: usize = 19;
const TAG_NUMBER_T61STRING: usize = 20;
const TAG_NUMBER_VIDEOTEXSTRING: usize = 21;
const TAG_NUMBER_IA5STRING: usize = 22;
const TAG_NUMBER_UTCTIME: usize = 23;
const TAG_NUMBER_GENERALIZEDTIME: usize = 24;
const TAG_NUMBER_GRAPHICSTRING: usize = 25;
const TAG_NUMBER_VISIBLESTRING: usize = 26;
const TAG_NUMBER_GENERALSTRING: usize = 27;
const TAG_NUMBER_UNIVERSALSTRING: usize = 28;
const TAG_NUMBER_CHARACTER_STRING: usize = 29;
const TAG_NUMBER_BMPSTRING: usize = 30;
const TAG_NUMBER_DATE: usize = 31;
const TAG_NUMBER_TIME_OF_DAY: usize = 32;
const TAG_NUMBER_DATE_TIME: usize = 33;
const TAG_NUMBER_DURATION: usize = 34;
const TAG_NUMBER_OID_IRI: usize = 35;
const TAG_NUMBER_RELATIVE_OID_IRI: usize = 36;

// TODO: It is important to ensure that we read all of the input till the end
// of each reader.

pub struct DERReader {
	remaining: Bytes,
	implicit_tag: Option<Tag>,
	next: Option<Element>
}

impl DERReader {
	pub fn new(input: Bytes) -> Self {
		Self { remaining: input, implicit_tag: None, next: None }
	}

	/// NOTE: The given number should be for the universal class.
	fn read_element(&mut self, class: TagClass, number: usize,
					constructed: bool) -> Result<Option<Bytes>> {
		let el =
			if let Some(e) = self.next.take() { e }
			else {
				let (e, rest) = Element::parse(self.remaining.clone())?;
				self.remaining = rest;
				e
			};

		let tag = self.implicit_tag.take().unwrap_or(Tag { class, number });
		
		if tag == el.ident.tag {
			if constructed != el.ident.constructed {
				return Err("Mismatch in P/C type".into());
			}

			Ok(Some(el.data))
		} else {
			self.next = Some(el);
			Ok(None)
		}
	}

	pub fn read_implicitly<T, F: FnMut(&mut DERReader) -> T>(
		&mut self, tag: Tag, mut f: F) -> T {
		self.implicit_tag = Some(tag);

		let ret = f(self);

		// This may be needed if the implicit tag was unused due to something
		// like an optional field.
		self.implicit_tag = None;
		
		ret
	}

	pub fn read_explicitly<T, F: FnMut(&mut DERReader) -> Result<Option<T>>>(
		&mut self, tag: Tag, mut f: F) -> Result<Option<T>> {

		let data = some_or_else!(self.read_element(
			tag.class, tag.number, true)?);
		
		let mut reader = DERReader::new(data);
		let v = match f(&mut reader)? {
			Some(v) => v,
			None => { return Err("Wrong type inside of explicit".into()); }
		};

		// TODO: Validate that we are always checking for this.
		if reader.remaining.len() > 0 {
			return Err("Explicitly typed object contains extra data".into());
		}

		Ok(Some(v))
	}

	pub fn read_bool(&mut self) -> Result<Option<bool>> {
		let data = some_or_else!(self.read_element(
			TagClass::Universal, TAG_NUMBER_BOOLEAN, false)?);

		if data.len() != 1 {
			Err("Data wrong size".into())
		} else if data[0] == 0x00 {
			Ok(Some(false))
		} else if data[0] == 0xff {
			Ok(Some(true))
		} else {
			Err("Invalid boolean value".into())
		}
	}

	pub fn read_int(&mut self) -> Result<Option<isize>> {
		let data = some_or_else!(self.read_element(
			TagClass::Universal, TAG_NUMBER_INTEGER, false)?);
		const ISIZE_LEN: usize = std::mem::size_of::<isize>();
		if data.len() < 1 || data.len() > ISIZE_LEN {
			return Err("Invalid data length".into());
		}

		let mut buf = if data[0] & 0x80 != 0 { [0xffu8; ISIZE_LEN] }
					  else { [0u8; ISIZE_LEN] };
		
		// TODO: Validate that it was minimal
		buf[(ISIZE_LEN - data.len())..].copy_from_slice(&data);

		let val = isize::from_be_bytes(buf);

		Ok(Some(val))
	}

	// pub fn write_bitstring(&mut self, bits: &BitString) {
	// 	let octets = bits.data.as_ref();
	// 	let nunused = octets.len() - bits.data.len();

	// 	self.write_tag(TagClass::Universal, TAG_NUMBER_BIT_STRING, false);
	// 	self.write_length(octets.len() + 1);
	// 	self.out.push(nunused as u8);
	// 	self.out.extend_from_slice(octets);
	// }

	pub fn read_octetstring(&mut self) -> Result<Option<OctetString>> {
		let data = some_or_else!(self.read_element(
			TagClass::Universal, TAG_NUMBER_OCTET_STRING, false)?);
		Ok(Some(OctetString { data }))
	}

	pub fn read_null(&mut self) -> Result<Option<()>> {
		let data = some_or_else!(self.read_element(
			TagClass::Universal, TAG_NUMBER_NULL, false)?);
		if data.len() != 0 {
			return Err("Expected no data in NULL".into());
		}

		Ok(Some(()))
	}

	pub fn read_sequence<T, F: Fn(&mut DERReader) -> Result<Option<T>>>(
		&mut self, extensible: bool, f: F) -> Result<Option<T>> {
		let data = some_or_else!(self.read_element(
			TagClass::Universal, TAG_NUMBER_SEQUENCE, true)?);

		let mut reader = Self::new(data);
		let out = f(&mut reader)?;

		if !extensible {
			reader.finished()?;
		}

		Ok(out)
	}

	pub fn read_sequence_of<T: DERReadable>(&mut self)
	-> Result<Option<Vec<T>>> {
		let data = some_or_else!(self.read_element(
			TagClass::Universal, TAG_NUMBER_SEQUENCE, true)?);
		
		let mut items = vec![];
		let mut reader = Self::new(data);
		while reader.remaining.len() > 0 {
			items.push(T::read_der(&mut reader)?);
		}
		
		Ok(Some(items))
	}


	/// Call at the end of parsing to ensure that all of the input was consumed.
	pub fn finished(&mut self) -> Result<()> {
		Ok(())
	}
}




/// NOTE: This object does not guranteed the DER rule that fields of a
/// SET/SEQUENCE with value equal to the default value are not included.
/// It is up to the caller to ensure that the appropriate method is not called
/// if this does happen.
pub struct DERWriter<'a> {
	out: &'a mut Vec<u8>,
	implicit_tag: Option<Tag>
}

// NOTE: Unless overriden, tag'ed types use the context-specific class
// NOTE: Writing a CHOICE that has an outer implicit tag effectively ends up
// being an explicit tag
impl<'a> DERWriter<'a> {
	pub fn new(out: &'a mut Vec<u8>) -> Self {
		Self { out, implicit_tag: None }
	}

	fn write_tag(&mut self, class: TagClass, number: usize, constructed: bool) {
		let tag = self.implicit_tag.take().unwrap_or(Tag { class, number });
		let ident = Identifier { tag, constructed };
		ident.serialize(self.out);
	}

	fn write_length(&mut self, len: usize) {
		Length::serialize(Some(len), self.out);
	}

	// TODO: It is important to ensure that write_implicitly and write_explicitly are only only used to write a single element in the callback.

	pub fn write_implicitly<F: FnMut(&mut DERWriter)>(
		&mut self, tag: Tag, mut f: F) {
		self.implicit_tag = Some(tag);

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
	pub fn write_int(&mut self, value: isize) {
		let data = value.to_be_bytes();
		let mut start_i = 0;
		while start_i < data.len() - 1 && (
			(data[start_i] == 0xff && data[start_i + 1] & 0x80 != 0) ||
			(data[start_i] == 0x00 && data[start_i + 1] & 0x80 == 0)
		) {
			start_i += 1;
		} 

		self.write_tag(TagClass::Universal, TAG_NUMBER_INTEGER, false);
		self.write_length(data.len() - start_i);
		self.out.extend_from_slice(&data[start_i..]);
	}

	/// In the primitive form, the first octet of the content is a u8 indicating
	/// the number of unused bits in the final. Bits are ordered from
	/// MSB to LSB.
	pub fn write_bitstring(&mut self, bits: &BitString) {
		let octets = bits.data.as_ref();
		let nunused = octets.len() - bits.data.len();

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

	// pub fn write_enum(&mut self, value: usize, out: &mut Vec<u8>) {
	// 	self.write_int(value, out);
	// }

	pub fn write_sequence<F: Fn(&mut DERWriter)>(&mut self, f: F) {
		let mut data = vec![];
		{
			let mut writer = Self::new(&mut data);
			f(&mut writer);
		}

		self.write_tag(TagClass::Universal, TAG_NUMBER_SEQUENCE, true);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	pub fn write_sequence_of<T: DERWriteable>(&mut self, items: &[T]) {
		let mut data = vec![];
		{
			let mut writer = Self::new(&mut data);
			for i in items {
				i.write_der(&mut writer);
			}
		}

		// TODO: Dedup this and the set_of one with the base write_sequence impl.
		self.write_tag(TagClass::Universal, TAG_NUMBER_SEQUENCE, true);
		self.write_length(data.len());
		self.out.extend_from_slice(&data);
	}

	pub fn write_set<F: Fn(&mut DERWriterIter)>(&mut self, f: F) {
		let mut iter = DERWriterIter::new();
		f(&mut iter);

		let mut elements = iter.into_inner().into_iter().filter_map(|v| {
			if v.len() == 0 {
				None
			} else {
				let data = Bytes::from(v);
				let (ident, _) = Identifier::parse(data.clone()).unwrap();
				Some((ident.tag, data))
			}
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
	
	pub fn write_set_of<T: DERWriteable>(&mut self, items: &[T]) {

		let mut iter = DERWriterIter::new();
		for i in items {
			i.write_der(&mut iter.next());
		}

		let mut elements = iter.into_inner();
		elements.sort();

		let len = elements.iter().fold(0, |v, data| v + data.len());

		self.write_tag(TagClass::Universal, TAG_NUMBER_SET, true);
		self.write_length(len);
		self.out.reserve(len);
		for data in elements {
			self.out.extend_from_slice(&data);
		}
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

	// DER Rules:
	// - For 'SET OF' the components are sorted based on the binary encoding value.
	// - If a default value is present, we should never encode any value equal to it.	

	// - For SET: "The canonical order for tags is based on the outermost tag of each type and is defined as follows:"

}

pub struct DERWriterIter {
	items: Vec<Vec<u8>>
}

impl DERWriterIter {
	pub fn new() -> Self {
		Self { items: vec![] }
	}

	pub fn next(&mut self) -> DERWriter {
		self.items.push(vec![]);
		// TODO: This writer should only be allowed to write a single tag.
		DERWriter::new(self.items.last_mut().unwrap())
	} 

	pub fn into_inner(mut self) -> Vec<Vec<u8>> {
		self.items
	}
}


pub trait DERWriteable {
	fn write_der(&self, writer: &mut DERWriter);
}

pub trait DERReadable: Sized {
	fn read_der(reader: &mut DERReader) -> Result<Self>;
}


