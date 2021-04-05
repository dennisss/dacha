use super::cyclic_buffer::*;
use crate::deflate::shared::*;
use crate::transform::TransformProgress;
use crate::huffman::*;
use byteorder::{LittleEndian, ReadBytesExt};
use common::bits::*;
use common::errors::*;
use std::io::Read;

/*
Some guidelines for compression:
- Don't compress unless we have at least 258 bytes of look ahead

Some guidelines for decompression
- Decompress single segment at a time.
- Can stop at any
*/

/// Top-level state for the Inflater state machine.
enum State {
    /// Waiting to read the next block's header.
    Start,

    /// Waiting to read the size of the uncompressed block.
    UncompressedHeader,

    /// In an uncompressed block reading raw bytes.
    UncompressedBlock {
        /// Number of bytes remaining to be read in this block.
        num_remaining: u16,
    },

    /// About to read HLIT, HDIST, HCLEN.
    DynamicBlockHeader,

    /// Have not yet read code length code lengths
    DynamicBlockCodeLenCodeLens {
        hlit: usize,
        hdist: usize,
        hclen: usize,
    },

    /// Reading main code lengths.
    DynamicBlockCodeLens {
        hlit: usize,
        hdist: usize,
        code_len_tree: HuffmanTree,
    },

    /// In a compressed block reading litlen/dist codes.
    CompressedBlockBody {
        litlen_tree: HuffmanTree,
        dist_tree: HuffmanTree,
        /// If this is set, then we are currently reading a
        pending_ref: Option<LenDist>,
    },
}

struct LenDist {
    len: usize,
    dist: usize,
}

struct BlockHeader {
    bfinal: bool,
    btype: u8,
}

struct OutputBuffer<'a> {
    buf: &'a mut [u8],
    index: usize,
}

enum ReadCodesResult {
    Done,
    NotDone,
    Reference(LenDist),
}

/*
    When we are given the full input, we want to just use a single output buffer (aka don't use any fancy chunking)
        ^ But still enforce the window size if it is fixed
*/

pub struct Inflater {
    state: State,

    final_seen: bool,

    /// Remaining bits from the compressed input which we have consumed but have
    /// not processed yet.
    input_prefix: BitVector,

    /// Stores the last N bytes of uncompressed data produced.
    output_window: CyclicBuffer,
}

impl Inflater {
    pub fn new() -> Self {
        Inflater {
            state: State::Start,
            final_seen: false,
            input_prefix: BitVector::new(),
            // TODO: Lazy allocate this memory as it is usually unneeded for small inputs.
            output_window: CyclicBuffer::new(MAX_REFERENCE_DISTANCE),
        }
    }

    /// NOTE: This assumes that all input is
    pub fn update(
        &mut self,
        input: &mut dyn Read, /* &[u8] */
        output: &mut [u8],
    ) -> Result<TransformProgress> {
        // let mut cursor = std::io::Cursor::new(&input);
        let cursor = input;
        let mut strm = BitReader::new(cursor);
        strm.load(self.input_prefix.clone())?; // TODO: Should delete the old value

        if self.is_done() {
            return Err(err_msg("Already done"));
        }

        let mut out = OutputBuffer {
            buf: output,
            index: 0,
        };

        while out.index < out.buf.len() {
            match self.update_inner(&mut strm, &mut out) {
                Ok(_) => {}
                Err(e) => {
                    if let Some(BitIoError::NotEnoughBits) = e.downcast_ref() {
                        break;
                    }

                    return Err(e);
                }
            };

            strm.consume();

            if self.is_done() {
                break;
            }
        }

        let done = self.is_done();
        if !done {
            self.output_window.extend_from_slice(&out.buf[0..out.index]);
            self.input_prefix = strm.into_unconsumed_bits();
        }

        // drop(strm);

        // let input_read = cursor.position();
        let output_written = out.index;

        // TODO: If the input is fully consumed without making any progress, then that
        // would be problematic.

        Ok(TransformProgress {
            input_read: 0, // TODO
            output_written,
            done,
        })
    }

    fn is_done(&self) -> bool {
        if self.final_seen {
            if let State::Start = &self.state {
                return true;
            }
        }

        false
    }

    /// This will attempt to advance the state machine once.
    /// If a BitIoErrorKind::NotEnoughBits is hit, then this operation should be
    /// able to be safely rolled back
    fn update_inner(&mut self, strm: &mut BitReader, out: &mut OutputBuffer) -> Result<()> {
        self.state = match &mut self.state {
            State::Start => {
                let header = self.read_block_header(strm)?;
                self.final_seen = header.bfinal;
                match header.btype {
                    // No compression
                    BTYPE_NO_COMPRESSION => State::UncompressedHeader,
                    // Compressed with fixed Huffman codes
                    BTYPE_FIXED_CODES => {
                        let litlen_tree = fixed_huffman_lenlit_tree()?;
                        let dist_tree = fixed_huffman_dist_tree()?;
                        State::CompressedBlockBody {
                            litlen_tree,
                            dist_tree,
                            pending_ref: None,
                        }
                    }
                    // Compressed with dynamic Huffman codes
                    BTYPE_DYNAMIC_CODES => State::DynamicBlockHeader,
                    _ => {
                        return Err(format_err!("Invalid BTYPE {}", header.btype));
                    }
                }
            }
            State::UncompressedHeader => {
                let len = self.read_uncompressed_header(strm)?;
                if len == 0 {
                    State::Start
                } else {
                    State::UncompressedBlock { num_remaining: len }
                }
            }
            State::UncompressedBlock { num_remaining } => {
                let n = std::cmp::min(out.buf.len() - out.index, *num_remaining as usize);

                // TODO: We should ensure try to ensure that this is never buffered as we don't need it to be.
                let nread = strm.read(&mut out.buf[out.index..(out.index + n)])?;
                out.index += nread;

                if nread == 0 {
                    return Err(BitIoError::NotEnoughBits.into());
                }

                let new_remaining = *num_remaining - (nread as u16);
                if new_remaining == 0 {
                    State::Start
                } else {
                    State::UncompressedBlock {
                        num_remaining: new_remaining,
                    }
                }
            }
            State::DynamicBlockHeader => {
                // TODO: Validate the maximum values for these.

                // Number of literal/length codes - 257.
                let hlit = strm.read_bits_exact(5)? + 257;
                // Number of distance codes - 1.
                let hdist = strm.read_bits_exact(5)? + 1;
                // Number of code length codes - 4
                let hclen = strm.read_bits_exact(4)? + 4;

                State::DynamicBlockCodeLenCodeLens { hlit, hdist, hclen }
            }
            State::DynamicBlockCodeLenCodeLens { hlit, hdist, hclen } => {
                // TODO: These can only be u8's?
                let mut code_len_code_lens = [0usize; 19];

                for i in 0..*hclen {
                    let l = strm.read_bits_exact(3)?;
                    code_len_code_lens[CODE_LEN_CODE_LEN_ORDERING[i] as usize] = l;
                }

                /*
                TODO:
                If only one distance
                code is used, it is encoded using one bit, not zero bits; in
                this case there is a single code length of one, with one unused
                code.  One distance code of zero bits means that there are no
                distance codes used at all (the data is all literals
                */

                let code_len_tree = HuffmanTree::from_canonical_lens(&code_len_code_lens)?;

                State::DynamicBlockCodeLens {
                    hlit: *hlit,
                    hdist: *hdist,
                    code_len_tree,
                }
            }
            State::DynamicBlockCodeLens {
                hlit,
                hdist,
                code_len_tree,
            } => {
                let all_lens = Self::read_dynamic_lens(strm, &code_len_tree, *hlit + *hdist)?;

                let litlen_tree = HuffmanTree::from_canonical_lens(&all_lens[0..*hlit])?;

                let dist_tree = HuffmanTree::from_canonical_lens(&all_lens[*hlit..])?;

                State::CompressedBlockBody {
                    litlen_tree,
                    dist_tree,
                    pending_ref: None,
                }
            }
            State::CompressedBlockBody {
                litlen_tree,
                dist_tree,
                pending_ref,
            } => {
                if let Some(litlen) = pending_ref {
                    *pending_ref = Self::read_reference(litlen, &self.output_window, out)?;
                    // No state change.
                    return Ok(());
                }

                match Self::read_block_codes(strm, &litlen_tree, &dist_tree, out)? {
                    ReadCodesResult::Done => State::Start,
                    ReadCodesResult::NotDone => {
                        return Ok(());
                    }
                    ReadCodesResult::Reference(r) => {
                        *pending_ref = Some(r);
                        return Ok(());
                    }
                }
            }
        };

        Ok(())
    }

    fn read_block_header(&mut self, strm: &mut BitReader) -> Result<BlockHeader> {
        Ok(BlockHeader {
            bfinal: strm.read_bits_exact(1)? != 0,
            btype: strm.read_bits_exact(2)? as u8,
        })
    }

    fn read_uncompressed_header(&mut self, strm: &mut BitReader) -> Result<u16> {
        // NOTE: The consume after align_to_byte() is only safe here because the caller of read_uncompressed_header didn't do any other reading on the byte before calling this function. So if we restart later, we will always be attempting to read the uncompressed header.
        strm.align_to_byte();
        strm.consume();

        let len = strm.read_u16::<LittleEndian>()?;
        let nlen = strm.read_u16::<LittleEndian>()?;
        if len != !nlen {
            return Err(err_msg("Uncompressed block lengths do not match"));
        }

        Ok(len)
    }

    fn read_dynamic_lens(
        strm: &mut BitReader,
        code_len_tree: &HuffmanTree,
        nsymbols: usize,
    ) -> Result<Vec<usize>> {
        let mut lens = vec![]; // TODO: Reserve elements.
        while lens.len() < nsymbols {
            let c = code_len_tree.read_code(strm)?;

            match c {
                0..=15 => {
                    lens.push(c);
                }
                16 => {
                    let n = 3 + (strm.read_bits(2)?.unwrap());
                    let l = *lens.last().unwrap();
                    for i in 0..n {
                        lens.push(l);
                    }
                }
                17 => {
                    let n = 3 + (strm.read_bits(3)?.unwrap());
                    // assert!(n <= 10);
                    for _ in 0..n {
                        lens.push(0);
                    }
                }
                18 => {
                    let n = 11 + (strm.read_bits(7)?.unwrap());
                    // assert!(n <= 138);
                    for i in 0..n {
                        lens.push(0);
                    }
                }
                _ => return Err(format_err!("Invalid code len code {}", c)),
            }
        }

        // This may not necessarily be true if repetition caused an overflow
        assert_eq!(nsymbols, lens.len());

        Ok(lens)
    }

    /// Returns whether or not the block is finished.
    fn read_block_codes(
        strm: &mut BitReader,
        litlen_tree: &HuffmanTree,
        dist_tree: &HuffmanTree,
        out: &mut OutputBuffer,
    ) -> Result<ReadCodesResult> {
        while out.index < out.buf.len() {
            let code = litlen_tree.read_code(strm)?;

            if code < END_OF_BLOCK {
                out.buf[out.index] = code as u8;
                out.index += 1;
                strm.consume();
            } else if code == END_OF_BLOCK {
                strm.consume();
                return Ok(ReadCodesResult::Done);
            } else {
                let len = read_len(code, strm)?;
                let dist_code = dist_tree.read_code(strm)?;
                let dist = read_distance(dist_code, strm)?;
                strm.consume();

                // Even if we have maintained enough bytes to resolve the reference, disallow
                // anything larger than the current window size.
                if dist > MAX_REFERENCE_DISTANCE {
                    return Err(err_msg("Distance larger than window size"));
                }

                return Ok(ReadCodesResult::Reference(LenDist { len, dist }));
            }
        }

        Ok(ReadCodesResult::NotDone)
    }

    /// This will produce the output for a given length/distance code.
    /// It will read the reference from a combination of the output window and
    /// the output buffer.
    fn read_reference(
        lendist: &LenDist,
        output_window: &CyclicBuffer,
        out: &mut OutputBuffer,
    ) -> Result<Option<LenDist>> {
        let mut len = lendist.len;
        let dist = lendist.dist;

        // TODO: Implement faster copy (make sure we retain ability to overlap with
        // output)

        // TODO: Perform all changes before entering the reference state.

        // Read from window.
        if dist > out.index {
            // Starting index
            let r = dist - out.index;
            if output_window.end_offset() - output_window.start_offset() < r {
                return Err(err_msg("Not enough bytes in window"));
            }

            // Copy from output window.
            let n = std::cmp::min(out.buf.len() - out.index, std::cmp::min(r, len));
            for i in 0..n {
                out.buf[out.index] = output_window[output_window.end_offset() - r + i];
                out.index += 1;
            }

            len -= n;
        }

        // Read from output buffer.
        if dist <= out.index {
            let start_i = out.index - dist;

            let n = std::cmp::min(len, out.buf.len() - out.index);
            for i in start_i..(start_i + n) {
                out.buf[out.index] = out.buf[i];
                out.index += 1;
            }

            len -= n;
        }

        Ok(if len == 0 {
            None
        } else {
            Some(LenDist { len, dist })
        })
    }
}

pub trait InflateRead {
    ///
    /// NOTE: This creates a new Inflater context each time so is not efficient
    /// to run multiple times.
    fn read_inflate(&mut self) -> Result<Vec<u8>>;
}

impl<T: Read> InflateRead for T {
    fn read_inflate(&mut self) -> Result<Vec<u8>> {
        let mut inflater = Inflater::new();

        let mut out = vec![];
        let mut out_size = 0;
        out.resize(4096, 0);

        // TODO: Currently this will never finish if we don't have enough input.
        loop {
            let progress = inflater.update(self, &mut out[out_size..])?;

            // Typically this will actually mean that a failure occured.
            // TODO: Get rid of this
            if progress.output_written == 0 {
                break;
            }

            out_size += progress.output_written;
            if progress.done {
                break;
            }

            out.resize(out_size + 4096, 0);
        }

        out.truncate(out_size);
        Ok(out)
    }
}

// reader.read_inflate();
