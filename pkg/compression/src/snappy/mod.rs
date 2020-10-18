// See https://github.com/google/snappy/blob/master/format_description.txt

use common::errors::*;
use protobuf::wire::{parse_varint, serialize_varint};

const TAG_TYPE_MASK: u8 = 0b11;
const TAG_LITERAL: u8 = 0b00;
const TAG_COPY1: u8 = 0b01;
const TAG_COPY2: u8 = 0b10;
const TAG_COPY4: u8 = 0b11;

fn byte(input: &[u8]) -> Result<(u8, &[u8])> {
    if input.len() < 1 {
        return Err(err_msg("Input too small"));
    }

    Ok((input[0], &input[1..]))
}

fn le_u16(input: &[u8]) -> Result<(u16, &[u8])> {
    if input.len() < 2 {
        return Err(err_msg("Input too small"));
    }

    let val = u16::from_le_bytes(*array_ref![input, 0, 2]);
    Ok((val, &input[2..]))
}

fn le_u32(input: &[u8]) -> Result<(u32, &[u8])> {
    if input.len() < 4 {
        return Err(err_msg("Input too small"));
    }

    let val = u32::from_le_bytes(*array_ref![input, 0, 4]);
    Ok((val, &input[4..]))
}

fn sized_usize(nbytes: usize) -> impl (Fn(&[u8]) -> Result<(usize, &[u8])>) {
    move |input| {
        if input.len() < nbytes {
            return Err(err_msg("Input too small"));
        }

        let mut buf = [0u8; std::mem::size_of::<usize>()];
        let (head, rest) = input.split_at(nbytes);
        buf[..nbytes].copy_from_slice(head);

        let val = usize::from_le_bytes(buf);
        Ok((val, rest))
    }
}

pub fn snappy_decompress<'a>(mut input: &'a [u8], output: &mut Vec<u8>) -> Result<&'a [u8]> {
    let uncompressed_length = parse_next!(input, parse_varint);
    output.reserve(uncompressed_length);

    let copy = |mut len: usize, offset: usize, output: &mut Vec<u8>| {
        if len == 0
            || offset == 0
            || offset > output.len()
            || output.len() + len > uncompressed_length
        {
            return Err(err_msg("Invalid len/offset pair"));
        }

        let mut pos = output.len();
        output.resize(output.len() + len, 0);

        while len > 0 {
            let n = std::cmp::min(offset, len);
            let start = pos - offset;
            let end = start + n;

            let (prev, next) = output.split_at_mut(pos);
            next[..n].copy_from_slice(&prev[start..end]);
            len -= n;
            pos += n;
        }

        Ok(())
    };

    while output.len() < uncompressed_length {
        let tag = parse_next!(input, byte);

        let tag_type = tag & TAG_TYPE_MASK;
        if tag_type == TAG_LITERAL {
            let tag_upper = tag >> 2;
            let size = if tag_upper >= 60 {
                // Number of bytes used to store the length.
                let len_size = (tag_upper - 59) as usize;
                parse_next!(input, sized_usize(len_size))
            } else {
                tag_upper as usize
            } + 1;

            if output.len() + size > uncompressed_length {
                return Err(err_msg("Literal exceeds expected uncompressed size"));
            }

            if input.len() < size {
                return Err(err_msg("Literal larger than remaining input"));
            }

            let (data, rest) = input.split_at(size);
            output.extend_from_slice(data);
            input = rest;
        } else if tag_type == TAG_COPY1 {
            let len = (((tag >> 2) & 0b111) + 4) as usize;
            // Upper 3 bits of the offset.
            let offset_upper = ((tag >> 5) & 0b111) as usize;
            let offset = (parse_next!(input, byte) as usize) | (offset_upper << 8);
            copy(len, offset, output)?;
        } else if tag_type == TAG_COPY2 {
            let len = ((tag >> 2) as usize) + 1;
            let offset = parse_next!(input, le_u16) as usize;
            copy(len, offset, output)?;
        } else if tag_type == TAG_COPY4 {
            let len = ((tag >> 2) as usize) + 1;
            let offset = parse_next!(input, le_u32) as usize;
            copy(len, offset, output)?;
        } else {
            panic!("");
        }
    }

    Ok(input)
}

// Compression is simple:
// - Look for closest match at most 64 bytes long and at least 4 bytes

// TODO: Must compress consecutive literal expressions together

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snappy_decompress_test() {
        let encoded = hex::decode(
            "51f05057696b697065646961206973206120667265652c207765622d6261736564\
			2c20636f6c6c61626f7261746976652c206d756c74696c696e6775616c20656e63\
			79636c6f70656469612070726f6a6563742e000000",
        )
        .unwrap();

        let mut decoded = vec![];
        let rest = snappy_decompress(&encoded, &mut decoded).unwrap();

        const expected: &'static [u8] =
            b"Wikipedia is a free, web-based, collaborative, multilingual \
			  encyclopedia project.";

        assert_eq!(&decoded[..], expected);
        assert_eq!(rest, &[0, 0, 0]);
    }
}
