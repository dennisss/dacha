// See https://github.com/google/snappy/blob/master/format_description.txt

// TODO: This code needs more error checking for the case of getting really
// large inputs.

use crate::deflate::cyclic_buffer::SliceBuffer;
use crate::deflate::matching_window::*;
use common::errors::*;
use parsing::binary::{le_u16, le_u32};
use protobuf::wire::{parse_varint, serialize_varint};

const TAG_TYPE_NBITS: u8 = 2;
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

fn sized_usize(nbytes: usize) -> impl (Fn(&[u8]) -> Result<(usize, &[u8])>) {
    move |input| {
        if input.len() < nbytes {
            // TODO: Return an incomplete_error.
            return Err(err_msg("Input too small"));
        }

        let mut buf = [0u8; std::mem::size_of::<usize>()];
        let (head, rest) = input.split_at(nbytes);
        buf[..nbytes].copy_from_slice(head);

        let val = usize::from_le_bytes(buf);
        Ok((val, rest))
    }
}

/// Decompresses the 'input' bytes into the 'output' buffer and returns any
/// remaining input data after the compressed part.
pub fn snappy_decompress<'a>(mut input: &'a [u8], output: &mut Vec<u8>) -> Result<&'a [u8]> {
    let uncompressed_length = parse_next!(input, parse_varint) as usize;
    output.reserve(uncompressed_length);

    // TODO: Generalize this code and use it for deflate as well.
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
            let tag_upper = tag >> TAG_TYPE_NBITS;
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
            let len = (((tag >> TAG_TYPE_NBITS) & 0b111) + 4) as usize;
            // Upper 3 bits of the offset.
            let offset_upper = ((tag >> 5) & 0b111) as usize;
            let offset = (parse_next!(input, byte) as usize) | (offset_upper << 8);
            copy(len, offset, output)?;
        } else if tag_type == TAG_COPY2 {
            let len = ((tag >> TAG_TYPE_NBITS) as usize) + 1;
            let offset = parse_next!(input, le_u16) as usize;
            copy(len, offset, output)?;
        } else if tag_type == TAG_COPY4 {
            let len = ((tag >> TAG_TYPE_NBITS) as usize) + 1;
            let offset = parse_next!(input, le_u32) as usize;
            copy(len, offset, output)?;
        } else {
            // This should never happen as we cover all 2-bit values above.
            panic!("Type larger by 2 bits");
        }
    }

    Ok(input)
}

fn snappy_write_reference(r: &RelativeReference, output: &mut Vec<u8>) {
    assert!(r.length <= 64);
    if r.length >= 4 && r.length <= 11 && r.distance <= 2047 {
        output.push(TAG_COPY1 | ((r.length - 4) << 2) as u8 | ((r.distance >> 8) << 5) as u8);
        output.push((r.distance & 0xff) as u8);
    } else if r.distance <= 65535 {
        output.push(TAG_COPY2 | ((r.length - 1) << 2) as u8);
        output.extend_from_slice(&(r.distance as u16).to_le_bytes())
    } else {
        output.push(TAG_COPY4 | ((r.length - 1) << 2) as u8);
        output.extend_from_slice(&(r.distance as u32).to_le_bytes());
    }
}

fn snappy_write_literal(data: &[u8], output: &mut Vec<u8>) {
    if data.len() == 0 {
        return;
    }

    let len = (data.len() - 1) as u32;
    if len <= 60 {
        output.push(TAG_LITERAL | ((len as u8) << TAG_TYPE_NBITS));
    } else {
        let nbytes = common::ceil_div(32 - len.leading_zeros() as usize, 8);
        output.push(TAG_LITERAL | ((59 + nbytes as u8) << TAG_TYPE_NBITS));

        let len_data = len.to_le_bytes();
        output.extend_from_slice(&len_data[0..nbytes]);
    }

    output.extend_from_slice(data);
}

pub fn snappy_compress(input: &[u8], output: &mut Vec<u8>) {
    serialize_varint(input.len() as u64, output);

    let mut window = MatchingWindow::new(
        SliceBuffer::new(input),
        MatchingWindowOptions {
            max_chain_length: 32,
            max_match_length: 64,
        },
    );

    // Index of the next byte index that needs to be compressed.
    let mut next_idx = 0;

    // Index of the current byte that we are looking to try using for matching.
    let mut i = 0;

    while i < input.len() {
        if let Some(r) = window.find_match(&input[i..]) {
            snappy_write_literal(&input[next_idx..i], output);

            snappy_write_reference(&r, output);

            window.advance(&input[i..(i + r.length)]);
            i += r.length;
            next_idx = i;
        } else {
            // Maybe add literal if we hit the u32 size limit for building literals?
            window.advance(&input[i..(i + 1)]);
            i += 1;
        }
    }

    snappy_write_literal(&input[next_idx..i], output);
}

// Store the tri-grams in the window.
// - Perform

// NOTE: The default snappy compressor uses 32768 byte large blocks.

// pub fn snappy_compress()

// Compression is simple:
// - Look for closest match at most 64 bytes long and at least 4 bytes

// TODO: Must compress consecutive literal expressions together

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snappy_decompress_test() {
        let encoded = common::hex::decode(
            "51f05057696b697065646961206973206120667265652c207765622d6261736564\
			2c20636f6c6c61626f7261746976652c206d756c74696c696e6775616c20656e63\
			79636c6f70656469612070726f6a6563742e000000",
        )
        .unwrap();

        let mut decoded = vec![];
        let rest = snappy_decompress(&encoded, &mut decoded).unwrap();

        const EXPECTED: &'static [u8] =
            b"Wikipedia is a free, web-based, collaborative, multilingual \
			  encyclopedia project.";

        assert_eq!(&decoded[..], EXPECTED);
        assert_eq!(rest, &[0, 0, 0]);
    }

    #[test]
    fn snappy_compress_test() {
        const INPUT: &'static [u8] =
            b"hello hello hello hello there you hello and this is super cool and awesome hello.";
        let mut compressed = vec![];
        snappy_compress(INPUT, &mut compressed);

        let mut uncompressed = vec![];
        let rest = snappy_decompress(&compressed, &mut uncompressed).unwrap();
        assert_eq!(rest.len(), 0);

        assert_eq!(INPUT, &uncompressed);
    }

    #[test]
    fn snappy_compress_test2() {
        let input = vec![0u8; 100000];

        let mut compressed = vec![];
        snappy_compress(&input, &mut compressed);

        println!("{}", compressed.len());

        let mut uncompressed = vec![];
        let rest = snappy_decompress(&compressed, &mut uncompressed).unwrap();
        assert_eq!(rest.len(), 0);

        assert_eq!(&input, &uncompressed);
    }
}
