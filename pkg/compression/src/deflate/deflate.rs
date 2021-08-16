/*
    Compression strategies:
    -
*/

use byteorder::{LittleEndian, WriteBytesExt};
use common::bits::*;
use common::errors::*;
use std::io::Write;

use crate::buffer_queue::BufferQueue;
use crate::deflate::cyclic_buffer::CyclicBuffer;
use crate::deflate::matching_window::{MatchingWindow, MatchingWindowOptions};
use crate::deflate::shared::*;
use crate::huffman::*;
use crate::transform::{Transform, TransformProgress};

/// Maximum size of the data contained in an uncompressed block.
const MAX_UNCOMPRESSED_BLOCK_SIZE: usize = u16::max_value() as usize;

// See https://github.com/madler/zlib/blob/master/deflate.c#L129 for all of zlib's parametes
const MAX_CHAIN_LENGTH: usize = 1024;

const MAX_MATCH_LENGTH: usize = 258;

// TODO: Never flush when not on an even byte position.

/// Once the compressor has received this many bytes, it will begin generating a
/// block. In zlib, this would be determined by the memLevel
const CHUNK_SIZE: usize = 8192;

/// All code length code lengths must fit in three bits (0-7).
const MAX_CODE_LEN_CODE_LEN: usize = 0b111;

// struct DeflateOptions {
// 	nice_match: u16,
// 	good_match: u16,
// 	lazy_match: u16

// }

/// A wrapper around a vector which can consumed (aka )
// struct ConsumableVec<T> {
// 	inner: Vec<T>,
// 	offset:
// }

// TODO: The zlib input threshold is based on number of encoded symbols rather
// than number of bits In order to add a

/// Compresses a stream of data using the Deflate algorithm.
pub struct Deflater {
    /// A sliding window of all previous input data that has already been
    /// compressed.
    window: MatchingWindow<CyclicBuffer>,

    /// Uncompressed input buffer. Will accumulate until we have enough to run
    /// compression
    input_buffer: Vec<u8>,

    /// Compressed data that has yet been consumed by the reader.
    /// This will grow indefinately until a client reads all data from the
    /// deflater.
    ///
    /// This is an Option so that a client can take the entire
    output_buffer: BufferQueue,

    /// Remainder of the last output_buffer byte which hasn't resulted in a full
    /// byte. This will always be [0, 8) bits long.
    output_buffer_end: BitVector,

    /// True if we've received an indication that all inputs
    /// TODO: Instead store the number of bytes so that we can validate this
    /// later.
    end_of_input: bool,
}

// TODO: Implement all of the zlib style signals
enum DeflateSignal {
    Flush,
    EndOfInput,
}

impl Deflater {
    // TODO: Provide window size as input option
    pub fn new() -> Self {
        Deflater {
            window: MatchingWindow::new(
                CyclicBuffer::new(MAX_REFERENCE_DISTANCE),
                MatchingWindowOptions {
                    max_chain_length: MAX_CHAIN_LENGTH,
                    max_match_length: MAX_MATCH_LENGTH,
                },
            ),
            input_buffer: vec![],
            output_buffer: BufferQueue::new(),
            output_buffer_end: BitVector::new(),
            end_of_input: false,
        }
    }

    /// Advances the state of the codec consuming some amount of the input and
    /// outputting transformed bytes into the provided output buffer.
    ///
    /// The first time this is called with input, it will accumulate the input
    /// in an internal buffer until enough is collected to start compression.
    /// Then the data in the internal buffer will be compressed and accumulated
    /// in an internal output buffer.
    ///
    /// If an output buffer is given to update(), then it will not consume any
    /// more input until the internal output buffer in clear. NOTE: If an
    /// empty output buffer is given, then this constraint will be ignored and
    /// output will be internally accumalated until the user consumes it.
    /// TODO: Instead use a configuration option tht defines whether the
    /// internal output buffer should have any bounded size.
    fn update_impl(
        &mut self,
        mut input: &[u8],
        end_of_input: bool,
        mut output: &mut [u8],
    ) -> Result<TransformProgress> {
        // TODO: Fix this. Also need to check for !end_of_input
        if end_of_input {
            if self.end_of_input || input.len() != 0 {
                return Err(err_msg("Received extra data after end of input hint"));
            }

            self.end_of_input = true;
        }

        // TODO: Once we are out of output space, stop compressing.

        let mut nread = 0;

        // Write buffered output from previous runs to output.
        let mut noutput = 0;
        if output.len() > 0 {
            noutput = self.output_buffer.copy_to(output);
            output = &mut output[noutput..];

            // let output_buffer_len = self.output_buffer.as_ref().map(|v|
            // v.len()).unwrap_or(0);

            // if output_buffer_len != 0 {
            //     // This is more output internally buffers. So stop.
            //     return Ok(Progress {
            //         input_read: 0,
            //         output_written: noutput,
            //         done: (end_of_input && ), // TODO
            //     });
            // }
        }

        // Setup stream
        let mut strm = BitWriter::new(&mut self.output_buffer.buffer);
        strm.write_bitvec(&self.output_buffer_end)?;

        // TODO: Enforce size limits on the output buffer

        // TODO: If the output buffer has space, try writing to it directly rather than
        // using the internal output uffer.

        // If we have previous input remaining, try to fill it to a full chunk.
        if self.input_buffer.len() > 0 {
            let n = std::cmp::min(CHUNK_SIZE - self.input_buffer.len(), input.len());
            self.input_buffer.extend_from_slice(&input[0..n]);
            input = &input[n..];
            nread += n;
            if self.input_buffer.len() == CHUNK_SIZE {
                Self::compress_chunk(
                    &mut self.window,
                    &self.input_buffer,
                    &mut strm,
                    end_of_input && input.len() == 0,
                )?;
                self.input_buffer.clear();
            }
            // TODO: Otherwise return straight away
        }

        // Compress full chunks from the provided buffer directly.
        let mut i = 0;
        let remaining = input.len() % CHUNK_SIZE;
        while i < input.len() - remaining {
            // Get a single chunk.
            // TODO: If using fixed huffman trees, it is easier to just keep concatenating
            // to a single block instead of making new blocks for each chunk?
            let n = CHUNK_SIZE;
            let chunk = &input[i..(i + n)];
            i += n;
            nread += n;

            Self::compress_chunk(&mut self.window, chunk, &mut strm, i == input.len())?
        }

        // Save remainder into the internal input buffer
        // TODO: This doesn't work as expected.

        // Handle remaining input data that doesn't align to a chunk boundary.
        // If all input data has been seen, compress an incomplete chunk, else store the
        //
        if remaining > 0 {
            nread += remaining;
            if end_of_input {
                Self::compress_chunk(&mut self.window, &input[i..], &mut strm, end_of_input)?;
            } else {
                self.input_buffer.extend_from_slice(&input[i..]);
            }
        }

        // TODO: Right here is the only time we should really need to copy bytes into
        // the matching window (not needed though if end_of_input)

        if end_of_input {
            strm.finish()?;
            // TODO: Reset all internal state
        }

        // Save remaining unfinished bits.
        self.output_buffer_end = strm.into_bits();

        // Copy output
        // TODO: Make this unwrap safer.
        noutput += self.output_buffer.copy_to(output);

        Ok(TransformProgress {
            input_read: nread, // NOTE: Currently we will always read the entire input given
            output_written: noutput,
            done: (end_of_input && self.output_buffer.is_empty()), // TODO
        })
    }

    /// Compresses a chunk of input data writing it to the given output stream.
    /// This will internally create a new full block for the chunk.
    fn compress_chunk(
        window: &mut MatchingWindow<CyclicBuffer>,
        chunk: &[u8],
        strm: &mut dyn BitWrite,
        is_final: bool,
    ) -> Result<()> {
        strm.write_bits(if is_final { 1 } else { 0 }, 1)?;
        strm.write_bits(BTYPE_DYNAMIC_CODES as usize, 2)?;

        // TODO: Ideally re-use a shared buffer for storing the intermediate
        // literals/lengths

        // Perform run length encoding.
        let codes = reference_encode(window, chunk);

        // Convert to atoms
        let mut atoms = vec![];
        for c in codes.into_iter() {
            match c {
                ReferenceEncoded::Literal(v) => {
                    append_lit(v, &mut atoms)?;
                }
                ReferenceEncoded::LengthDistance { length, distance } => {
                    append_len(length, &mut atoms)?;
                    append_distance(distance, &mut atoms)?;
                }
            }
        }

        append_end_of_block(&mut atoms);

        // Build huffman trees (will need to partially extract codes)
        let mut litlen_symbols = vec![];
        let mut dist_symbols = vec![];
        for a in atoms.iter() {
            match a {
                Atom::LitLenCode(c) => litlen_symbols.push(*c),
                Atom::DistCode(c) => dist_symbols.push(*c),
                Atom::ExtraBits(_) => {}
            };
        }

        let mut litlen_lens = dense_symbol_lengths(&HuffmanTree::build_length_limited_tree(
            &litlen_symbols,
            MAX_LITLEN_CODE_LEN,
        )?);
        if litlen_lens.len() < 257 {
            litlen_lens.resize(257, 0);
        }

        let hlit = litlen_lens.len() - 257;
        strm.write_bits(hlit, 5)?;

        let mut dist_lens = dense_symbol_lengths(&HuffmanTree::build_length_limited_tree(
            &dist_symbols,
            MAX_LITLEN_CODE_LEN,
        )?);
        if dist_lens.len() < 1 {
            dist_lens.resize(1, 0);
        }

        let hdist = dist_lens.len() - 1;
        strm.write_bits(hdist, 5)?;

        let mut code_lens = vec![];
        code_lens.extend_from_slice(&litlen_lens);
        code_lens.extend_from_slice(&dist_lens);

        let code_len_atoms = append_dynamic_lens(&code_lens)?;
        let code_len_symbols = code_len_atoms
            .iter()
            .filter_map(|a| match a {
                CodeLengthAtom::Symbol(u) => Some(*u as usize),
                _ => None,
            })
            .collect::<Vec<_>>();

        let sparse_code_len_code_lens =
            &HuffmanTree::build_length_limited_tree(&code_len_symbols, MAX_CODE_LEN_CODE_LEN)?;

        // Reorder the lengths and write to stream.
        {
            let mut ordering_inv = [0u8; CODE_LEN_ALPHA_SIZE];
            for (i, v) in CODE_LEN_CODE_LEN_ORDERING.iter().enumerate() {
                ordering_inv[*v as usize] = i as u8;
            }

            // TODO: There should be no need to do comparisons as we know the offsets
            let mut reordered = [0u8; CODE_LEN_ALPHA_SIZE];
            let mut reordered_len = 0;
            for v in sparse_code_len_code_lens.iter() {
                let i = ordering_inv[v.symbol] as usize;
                reordered[i] = v.length as u8;
                reordered_len = std::cmp::max(reordered_len, i + 1);
            }

            if reordered_len < 4 {
                reordered_len = 4;
            }

            let hclen = reordered_len - 4;
            strm.write_bits(hclen, 4)?;

            for len in &reordered[0..reordered_len] {
                strm.write_bits(*len as usize, 3)?;
            }
        }

        let code_len_code_lens = dense_symbol_lengths(sparse_code_len_code_lens);

        let code_len_encoder = HuffmanEncoder::from_canonical_lens(&code_len_code_lens)?;

        for atom in code_len_atoms.into_iter() {
            match atom {
                CodeLengthAtom::Symbol(s) => {
                    code_len_encoder.write_symbol(s as usize, strm)?;
                }
                CodeLengthAtom::ExtraBits(v) => {
                    strm.write_bitvec(&v)?;
                }
            };
        }

        let litlen_encoder = HuffmanEncoder::from_canonical_lens(&litlen_lens)?;
        let dist_encoder = HuffmanEncoder::from_canonical_lens(&dist_lens)?;

        // Now write the actual data in this block
        for atom in atoms.into_iter() {
            match atom {
                Atom::LitLenCode(c) => litlen_encoder.write_symbol(c, strm)?,
                Atom::DistCode(c) => dist_encoder.write_symbol(c, strm)?,
                Atom::ExtraBits(v) => strm.write_bitvec(&v)?,
            };
        }

        // TODO: If the histogram is sufficiently similar to the fixed tree one, then
        // use the fixed tree

        // TODO: If the histogram and number of run-length encoded bytes is sufficiently
        // small, use a heuristic to move to a no compression block instead

        Ok(())
    }

    // /// Returns all data in the internal output buffer. This is a zero copy
    // /// operation and will leave the internal buffer empty.
    // pub fn take_output(&mut self) -> Vec<u8> {
    //     // TODO: Don't allow

    //     // TODO: This should only take data after the buffer offset.
    //     self.output_buffer.take().unwrap_or(BufferQueue::new()).buffer
    // }
}

impl Transform for Deflater {
    fn update(
        &mut self,
        input: &[u8],
        end_of_input: bool,
        output: &mut [u8],
    ) -> Result<TransformProgress> {
        self.update_impl(input, end_of_input, output)
    }
}

// NOTE: Assumes that the header has already been written
fn write_uncompressed_block<W: BitWrite + Write>(data: &[u8], strm: &mut W) -> Result<()> {
    if data.len() > MAX_UNCOMPRESSED_BLOCK_SIZE {
        return Err(err_msg("Data too long for uncompressed block"));
    }

    let l = data.len() as u16;
    strm.write_u16::<LittleEndian>(l)?;
    strm.write_u16::<LittleEndian>(!l)?;

    // TODO: Don't write if we consume the entire output buffer.
    strm.write_all(data)?;
    Ok(())
}

// Given a some data, can we tree to run length encode it

enum ReferenceEncoded {
    Literal(u8),
    LengthDistance { length: usize, distance: usize },
}

// TODO: Return an iterator.
fn reference_encode(
    window: &mut MatchingWindow<CyclicBuffer>,
    data: &[u8],
) -> Vec<ReferenceEncoded> {
    let mut out = vec![];

    let mut i = 0;
    while i < data.len() {
        let mut n = 1;
        if let Some(m) = window.find_match(&data[i..]) {
            n = m.length;
            out.push(ReferenceEncoded::LengthDistance {
                length: m.length,
                distance: m.distance,
            });
        } else {
            out.push(ReferenceEncoded::Literal(data[i]));
        }

        // TODO: Usually we should not be copying bytes until the very end of the
        // current chunk
        window.advance(&data[i..(i + n)]);

        i += n;
    }

    assert_eq!(i, data.len());

    out
}

#[derive(Debug)]
enum CodeLengthAtom {
    Symbol(u8),
    ExtraBits(BitVector),
}

// TODO: If the lens list ends in 0's then we don't really need to encode it
/// Given the encoded code lengths for the dynamic length/literal and distance
/// code trees, this will encode/compress them into the code length alphabet and
/// write them to the output stream
fn append_dynamic_lens(lens: &[usize]) -> Result<Vec<CodeLengthAtom>> {
    let mut out = vec![];

    // We can only encode code lengths up to 15.
    for len in lens {
        if *len > MAX_LITLEN_CODE_LEN {
            return Err(err_msg("Length is too long"));
        }
    }

    let mut i = 0;
    while i < lens.len() {
        let v = lens[i];

        // Look for a sequence of zeros.
        if v == 0 {
            let mut j = i + 1;
            let j_max = std::cmp::min(lens.len(), i + 138);
            while j < lens.len() && lens[j] == 0 {
                j += 1;
            }

            let n = j - i;
            if n >= 11 {
                out.push(CodeLengthAtom::Symbol(18));
                out.push(CodeLengthAtom::ExtraBits(BitVector::from_usize(n - 11, 7)));

                i += n;
                continue;
            } else if n >= 3 {
                out.push(CodeLengthAtom::Symbol(17));
                out.push(CodeLengthAtom::ExtraBits(BitVector::from_usize(n - 3, 3)));

                i += n;
                continue;
            }
        }

        // Look for a sequence of repeated lengths
        if i > 0 && lens[i - 1] == v {
            let mut j = i + 1;
            let j_max = std::cmp::min(lens.len(), i + 6); // We can only encode up to 6 repetitions
            while j < j_max && lens[j] == v {
                j += 1;
            }

            let n = j - i;
            if n >= 3 {
                out.push(CodeLengthAtom::Symbol(16));
                out.push(CodeLengthAtom::ExtraBits(BitVector::from_usize(n - 3, 2)));

                i += n;
                continue;
            }
        }

        // Otherwise, just encode as a plain length
        out.push(CodeLengthAtom::Symbol(v as u8));
        i += 1;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_dynamic_lens_test() {
        let input = [0, 0, 0, 0, 3, 4, 5, 5, 5, 5, 5, 5, 12, 5];
        let out = append_dynamic_lens(&input).unwrap();
        println!("{:?}", out);
    }
}
