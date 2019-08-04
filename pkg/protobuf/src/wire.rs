use super::{Message, Enum};
use byteorder::{ByteOrder, LittleEndian};
use bytes::{BytesMut, Bytes};
use std::intrinsics::unlikely;

type Result<T> = std::result::Result<T, &'static str>;

fn serialize_varint(mut v: usize, out: &mut Vec<u8>) {
	loop {
		let mut b = (v & 0x7f) as u8;
		v = v >> 7;
		if v != 0 {
			b &= 0x80;
			out.push(b);
		} else {
			out.push(b);
			break;
		}
	}
}

fn parse_varint(input: &[u8]) -> Result<(usize, &[u8])> {
	let mut v = 0;
	let mut i = 0;
	
	// Maximum number of bytes to take.
	// Limited by size of input and size of 64bit integer.
	let max_bytes = std::cmp::min(input.len(), 64 / 7);
	
	loop {
		let overflow = i >= max_bytes;
		if unsafe { unlikely(overflow) } {
			return Err("To few/many bytes in varint");
		}

		let mut b = input[i] as usize;
		let more = b & 0x80 != 0;
		b = b & 0x7f;

		v |= b << (7*i);

		// Consume byte.
		i += 1;

		if !more {
			break;
		}
	}

	Ok((v, &input[i..]))
}

fn encode_zigzag32(n: usize) -> usize { (n << 1) ^ (n >> 31) }
fn encode_zigzag64(n: usize) -> usize { (n << 1) ^ (n >> 63) }

#[derive(Clone, Copy)]
enum WireType {
	Varint = 0,
	Word64 = 1,
	LengthDelim = 2,
	StartGroup = 3,
	EndGroup = 4,
	Word32 = 5
}

impl WireType {
	fn from_usize(v: usize) -> Result<WireType> {
		Ok(match v {
			0 => WireType::Varint,
			1 => WireType::Word64,
			2 => WireType::LengthDelim,
			3 => WireType::StartGroup,
			4 => WireType::EndGroup,
			5 => WireType::Word32,
			_ => { return Err("Invalid wire type number"); }
		})
	}
}

struct Tag {
	field_number: usize,
	wire_type: WireType
}

impl Tag {
	fn parse(input: &[u8]) -> Result<(Tag, &[u8])> {
		let (v, rest) = parse_varint(input)?;
		let wire_type = WireType::from_usize(v & 0b111)?;
		let field_number = v >> 3;
		Ok((Tag { field_number, wire_type }, rest))
	}

	// TODO: Ensure field_number is within the usize range
	fn serialize(&self, out: &mut Vec<u8>) {
		let v = (self.field_number << 3) | (self.wire_type as usize);
		serialize_varint(v, out);
	}
}

pub struct WireField<'a> {
	pub field_number: usize,
	pub value: WireValue<'a>
}

impl WireField<'_> {
	pub fn serialize_double(field_number: usize, v: f64, out: &mut Vec<u8>) {
		let mut buf = [0u8; 8]; 
		LittleEndian::write_f64(&mut buf, v);
		Tag { field_number, wire_type: WireType::Word64 }.serialize(out);
		WireValue::Word64(&buf).serialize(out);
	}

	pub fn serialize_float(field_number: usize, v: f32, out: &mut Vec<u8>) {
		let mut buf = [0u8; 4]; 
		LittleEndian::write_f32(&mut buf, v);
		Tag { field_number, wire_type: WireType::Word32 }.serialize(out);
		WireValue::Word32(&buf).serialize(out);
	}

	pub fn serialize_int32(field_number: usize, v: i32, out: &mut Vec<u8>) {
		Tag { field_number, wire_type: WireType::Varint }.serialize(out);
		WireValue::serialize_int32(v, out)
	}
}


pub enum WireValue<'a> {
	Varint(usize), // sint32, sint64, bool, enum
	Word64(&'a [u8]), // fixed64, sfixed64
	LengthDelim(&'a [u8]), // bytes, embedded messages, packed repeated fields
	Group(Vec<WireValue<'a>>),
	Word32(&'a [u8])
}

macro_rules! enum_accessor {
	($name:ident, $branch:ident, $t:ty) => {
		fn $name(&self) -> Result<$t> {
			if let Self::$branch(v) = self { Ok(*v) }
			else { Err("Unexpected value type.") }
		}
	};
}

impl WireValue<'_> {
	enum_accessor!(varint, Varint, usize);
	enum_accessor!(word64, Word64, &[u8]);
	enum_accessor!(length_delim, LengthDelim, &[u8]);
	enum_accessor!(word32, Word32, &[u8]);
	
	fn serialize(&self, out: &mut Vec<u8>) {
		match self {
			WireValue::Varint(n) => serialize_varint(*n, out),
			WireValue::Word64(v) => out.extend_from_slice(&v),
			WireValue::LengthDelim(v) => {
				serialize_varint(v.len(), out);
				out.extend_from_slice(v);
			},
			WireValue::Group(items) => {
				for i in items {
					i.serialize(out);
				}
			},
			WireValue::Word32(v) => out.extend_from_slice(v)
		};
	}

	pub fn parse_double(&self) -> Result<f64> {
		Ok(LittleEndian::read_f64(self.word64()?))
	}
	fn serialize_double(v: f64, out: &mut Vec<u8>) {
		let mut buf = [0u8; 8]; 
		LittleEndian::write_f64(&mut buf, v);
		Self::Word64(&buf).serialize(out);
	}

	pub fn parse_float(&self) -> Result<f32> {
		Ok(LittleEndian::read_f32(self.word32()?))
	}
	fn serialize_float(v: f32, out: &mut Vec<u8>) {
		let mut buf = [0u8; 4]; 
		LittleEndian::write_f32(&mut buf, v);
		Self::Word32(&buf).serialize(out);
	}

	pub fn parse_int32(&self) -> Result<i32> { Ok(self.varint()? as i32) }
	fn serialize_int32(v: i32, out: &mut Vec<u8>) {
		Self::Varint(v as usize).serialize(out);
	}

	pub fn parse_int64(&self) -> Result<i64> { Ok(self.varint()? as i64) }
	fn serialize_int64(v: i64, out: &mut Vec<u8>) {
		Self::Varint(v as usize).serialize(out);
	}

	pub fn parse_uint32(&self) -> Result<u32> { Ok(self.varint()? as u32) }
	fn serialize_uint32(v: u32, out: &mut Vec<u8>) {
		Self::Varint(v as usize).serialize(out);
	}

	pub fn parse_uint64(&self) -> Result<u64> { Ok(self.varint()? as u64) }
	fn serialize_uint64(v: u64, out: &mut Vec<u8>) {
		Self::Varint(v as usize).serialize(out);
	}

	// parse_sint32
	// parse_sint64

	pub fn parse_fixed32(&self) -> Result<u32> {
		Ok(LittleEndian::read_u32(self.word32()?))
	}
	pub fn parse_fixed64(&self) -> Result<u64> {
		Ok(LittleEndian::read_u64(self.word64()?))
	}
	pub fn parse_sfixed32(&self) -> Result<i32> {
		Ok(LittleEndian::read_i32(self.word32()?))
	}
	pub fn parse_sfixed64(&self) -> Result<i64> {
		Ok(LittleEndian::read_i64(self.word64()?))
	}

	pub fn parse_bool(&self) -> Result<bool> { Ok(self.varint()? != 0) }
	fn serialize_bool(v: bool, out: &mut Vec<u8>) {
		Self::Varint(if v { 1 } else { 0 }).serialize(out);
	}


	pub fn parse_string(&self) -> Result<String> {
		let mut val = vec![];
		val.extend_from_slice(self.length_delim()?);
		String::from_utf8(val).map_err(|_| "Invalid utf-8 bytes in string")
	}

	// parse_bytes
	pub fn parse_bytes(&self) -> Result<BytesMut> {
		let mut val = vec![];
		val.extend_from_slice(self.length_delim()?);
		Ok(BytesMut::from(val))
	}

	pub fn parse_enum<E: Enum>(&self) -> Result<E> {
		E::from_usize(self.varint()?)
	}

	pub fn parse_message<M: Message>(&self) -> Result<M> {
		let data = self.length_delim()?;
		M::parse(Bytes::from(data))
	}
	pub fn serialize_message<M: Message>(m: M, out: &mut Vec<u8>) {
		let data = m.serialize();
		Self::LengthDelim(&data).serialize(out);
	}
}

pub fn parse_wire(mut input: &[u8]) -> Result<Vec<WireField>> {
	let mut out = vec![];
	let mut group = None;

	while input.len() > 0 {
		let (tag, rest) = Tag::parse(input)?;
		input = rest;
		let value = match tag.wire_type {
			WireType::Varint => {
				let (v, rest) = parse_varint(input)?;
				input = rest;
				WireValue::Varint(v)
			},
			WireType::Word64 => {
				if input.len() < 8 { return Err("Too few bytes for word64"); }
				let v = &input[0..8];
				input = &input[8..];
				WireValue::Word64(v)
			},
			WireType::LengthDelim => {
				let (len, rest) = parse_varint(input)?;
				input = rest;
				if input.len() < len { return Err("Too few bytes for length delimited"); }
				let v = &input[0..len];
				input = &input[len..];
				WireValue::LengthDelim(v)
			},
			WireType::StartGroup => {
				group = Some(vec![]);
				continue;
			},
			WireType::EndGroup => {
				let v = match group.take() {
					Some(items) => WireValue::Group(items),
					None => { return Err("Saw EndGroup before seeing a StartGroup"); }
				};

				v
			},
			WireType::Word32 => {
				if input.len() < 4 { return Err("Too few bytes for word32"); }
				let v = &input[0..4];
				input = &input[4..];
				WireValue::Word32(v)
			}
		};

		out.push(WireField { field_number: tag.field_number, value });
	}

	if input.len() == 0 {
		if group.is_some() {
			return Err("Unclosed group with no input remaining.");
		}

		Ok(out)
	} else {
		// This should pretty much never happen due to the while loop above
		Err("Could not parse all input")
	}
}




