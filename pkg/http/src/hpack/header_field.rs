use common::bytes::Bytes;
use common::errors::*;
use parsing::binary::be_u8;
use parsing::parse_next;

use crate::hpack::primitive::*;

/// Owned header key value pair.
/// Both the name and value as opaque byte strings.
#[derive(Clone, PartialEq, Debug)]
pub struct HeaderField {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
}

/// Referenced version of a HeaderField.
#[derive(Clone, Copy)]
pub struct HeaderFieldRef<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
}

impl HeaderFieldRef<'_> {
    pub fn to_owned(&self) -> HeaderField {
        HeaderField {
            name: self.name.to_owned(),
            value: self.value.to_owned(),
        }
    }
}

impl<'a> std::convert::From<&'a HeaderField> for HeaderFieldRef<'a> {
    fn from(header: &'a HeaderField) -> Self {
        Self {
            name: &header.name,
            value: &header.value,
        }
    }
}

/// The binary representation of a header or dynamic size update command.
#[derive(Debug)]
pub enum HeaderFieldRepresentation<'a, 'b> {
    // NOTE: The value can't be Indexed unless the name is also indexed.
    HeaderField {
        name: StringReference<'a>,
        value: StringReference<'b>,
        indexed: IndexingMode,
    },

    DynamicTableSizeUpdate {
        max_size: usize,
    },
}

impl HeaderFieldRepresentation<'_, '_> {
    pub fn parse(mut input: &[u8]) -> Result<(HeaderFieldRepresentation, &[u8])> {
        let (first_byte, _) = be_u8(input)?;

        let instance = match first_byte.leading_zeros() {
            // Starts with '1'
            // RFC 7531: Section 6.1
            0 => {
                let index = {
                    let (v, rest) = parse_varint(input, 7)?;
                    input = rest;
                    v
                };

                if index == 0 {
                    return Err(err_msg("Unexpected zero index"));
                }

                HeaderFieldRepresentation::HeaderField {
                    name: StringReference::Indexed(index),
                    value: StringReference::Indexed(index),
                    indexed: IndexingMode::No,
                }
            }
            // Starts with '01'
            // RFC 7531: Section 6.2.1
            1 => {
                parse_next!(
                    input,
                    HeaderFieldRepresentation::parse_literal_header(6, IndexingMode::Yes)
                )
            }
            // Starts with '001'
            // RFC 7531: Section 6.3
            2 => {
                let max_size = {
                    let (v, rest) = parse_varint(input, 5)?;
                    input = rest;
                    v
                };

                HeaderFieldRepresentation::DynamicTableSizeUpdate { max_size }
            }
            // Starts with '0001'
            // RFC 7531: Section 6.2.3
            3 => {
                parse_next!(
                    input,
                    HeaderFieldRepresentation::parse_literal_header(4, IndexingMode::Never)
                )
            }
            // Starts with '0000'
            // RFC 7531: Section 6.2.2
            _ => {
                parse_next!(
                    input,
                    HeaderFieldRepresentation::parse_literal_header(4, IndexingMode::No)
                )
            }
        };

        Ok((instance, input))
    }

    fn parse_literal_header(
        code_prefix_bits: usize,
        indexed: IndexingMode,
    ) -> impl Fn(&[u8]) -> Result<(HeaderFieldRepresentation, &[u8])> {
        move |mut input| {
            let index = {
                let (v, rest) = parse_varint(input, code_prefix_bits)?;
                input = rest;
                v
            };

            let name = {
                if index == 0 {
                    StringReference::Literal(parse_next!(input, parse_string_literal))
                } else {
                    StringReference::Indexed(index)
                }
            };

            let value = StringReference::Literal(parse_next!(input, parse_string_literal));

            Ok((
                HeaderFieldRepresentation::HeaderField {
                    name,
                    value,
                    indexed,
                },
                input,
            ))
        }
    }

    pub fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {
        let first_i = out.len();
        match self {
            HeaderFieldRepresentation::HeaderField {
                name,
                value,
                indexed,
            } => {
                match indexed {
                    IndexingMode::No => {
                        // Special case when both are indexed.
                        if let StringReference::Indexed(name_idx) = name {
                            if let StringReference::Indexed(value_idx) = value {
                                if *name_idx != *value_idx {
                                    return Err(err_msg(
                                        "name/value indexed with different indices",
                                    ));
                                }

                                if *name_idx == 0 {
                                    return Err(err_msg("Zero index"));
                                }

                                serialize_varint(*name_idx, 7, out);
                                out[first_i] |= 1 << 7;
                                return Ok(());
                            }
                        }

                        HeaderFieldRepresentation::serialize_literal_header(
                            4, *indexed, name, value, out,
                        )?;
                        // NOTE: Mask is 0
                    }
                    IndexingMode::Yes => {
                        HeaderFieldRepresentation::serialize_literal_header(
                            6, *indexed, name, value, out,
                        )?;
                        out[first_i] |= 0b01 << 6;
                    }
                    IndexingMode::Never => {
                        HeaderFieldRepresentation::serialize_literal_header(
                            4, *indexed, name, value, out,
                        )?;
                        out[first_i] |= 0b0001 << 4;
                    }
                }
            }
            HeaderFieldRepresentation::DynamicTableSizeUpdate { max_size } => {
                serialize_varint(*max_size, 5, out);
                out[first_i] |= 0b001 << 5;
            }
        };

        Ok(())
    }

    fn serialize_literal_header(
        code_prefix_bits: usize,
        indexed: IndexingMode,
        name: &StringReference,
        value: &StringReference,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        let value_data = match value {
            StringReference::Literal(data) => &*data,
            StringReference::LiteralRef(data) => *data,
            StringReference::Indexed(_) => {
                return Err(err_msg("Literal header field can't have an indexed value"));
            }
        };

        let maybe_compress = indexed != IndexingMode::Never;

        match name {
            StringReference::Indexed(name_idx) => {
                if *name_idx == 0 {
                    return Err(err_msg("Zero index"));
                }

                serialize_varint(*name_idx, code_prefix_bits, out);
            }
            StringReference::Literal(name_data) => {
                serialize_varint(0, code_prefix_bits, out);
                serialize_string_literal(&name_data, maybe_compress, out);
            }
            StringReference::LiteralRef(name_data) => {
                // TODO: Dedup with the Literal case.
                serialize_varint(0, code_prefix_bits, out);
                serialize_string_literal(*name_data, maybe_compress, out);
            }
        };

        serialize_string_literal(&value_data, maybe_compress, out);

        Ok(())
    }
}

#[derive(Debug)]
pub enum StringReference<'a> {
    Indexed(usize),
    Literal(Vec<u8>),

    // TODO: Consider supporting parsing as this when not compressed.
    LiteralRef(&'a [u8]),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IndexingMode {
    No,

    /// The field value will not be indexed and will not be compressed using
    /// huffman coding.
    Never,

    Yes,
}
