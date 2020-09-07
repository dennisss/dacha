use crate::{Colorspace, Image};
use byteorder::{BigEndian, ReadBytesExt};
use common::bits::{BitOrder, BitReader, BitVector};
use common::ceil_div;
use common::errors::*;
use common::futures::future::err;
use compression::huffman::HuffmanTree;
use math::array::Array;
use math::matrix::Dimension;
use parsing::binary::{be_u16, be_u8};
use parsing::take_exact;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};

const START_OF_IMAGE: &[u8] = &[0xff, 0xd8]; // SOI
const END_OF_IMAGE: u8 = 0xd9; // EOI

const APP0: u8 = 0xe0;

// Start Of Frame markers, non-differential, Huffman coding
const SOF0: u8 = 0xC0; // Baseline DCT
const SOF1: u8 = 0xC1; // Extended sequential DCT
const SOF2: u8 = 0xC2; // Progressive DCT
const SOF3: u8 = 0xC3; // Lossless (sequential)

// Define Arithmetic Coding Conditioning Table(s)
const DAC: u8 = 0xCC;

// Define Huffman Table
const DHT: u8 = 0xC4;

// Define Quantization Table
const DQT: u8 = 0xDB;

/// Define Restart Interval
const DRI: u8 = 0xDD;

const START_OF_SCAN: u8 = 0xda; // SOS

// TODO: In sequential and lossless modes, this can be up to 255
const MAX_NUM_COMPONENTS: usize = 4;

#[derive(Debug, PartialEq)]
enum DCTMode {
    Baseline,
    Extended,
    Progressive,
    Lossless,
}

/*
References:
https://en.wikipedia.org/wiki/JPEG_File_Interchange_Format
https://www.w3.org/Graphics/JPEG/itu-t81.pdf
https://www.w3.org/Graphics/JPEG/jfif3.pdf

See here for more test images:
https://www.w3.org/MarkUp/Test/xhtml-print/20050519/tests/A_2_1-BF-01.htm
*/

const BLOCK_DIM: usize = 8;
const BLOCK_SIZE: usize = 64;

const ZIG_ZAG_SEQUENCE: &[u8; BLOCK_SIZE] = &[
    0, 1, 5, 6, 14, 15, 27, 28, //
    2, 4, 7, 13, 16, 26, 29, 42, //
    3, 8, 12, 17, 25, 30, 41, 43, //
    9, 11, 18, 24, 31, 40, 44, 53, //
    10, 19, 23, 32, 39, 45, 52, 54, //
    20, 22, 33, 38, 46, 51, 55, 60, //
    21, 34, 37, 47, 50, 56, 59, 61, //
    35, 36, 48, 49, 57, 58, 62, 63, //
];

fn apply_zigzag<T: Copy>(inputs: &[T], outputs: &mut [T]) {
    for i in 0..inputs.len() {
        outputs[ZIG_ZAG_SEQUENCE[i] as usize] = inputs[i];
    }
}

fn reverse_zigzag<T: Copy>(inputs: &[T], outputs: &mut [T]) {
    for i in 0..inputs.len() {
        outputs[i] = inputs[ZIG_ZAG_SEQUENCE[i] as usize];
    }
}

// TODO: These can be very large. Check that they don't cause out of range
// multiplications. NOTE: Only works if size < 16.
// TODO: Rename decode_amplitude?
fn decode_zz(size: usize, amplitude: u16) -> i16 {
    let sign = (amplitude >> ((size as u16) - 1)) & 0b11;
    if sign == 1 {
        // It is positive
        return amplitude as i16;
    }

    let extended = (0xffff_u16).overflowing_shl(size as u32).0 | amplitude;

    (extended as i16) + 1
}

/*
TODO: Read the 'Practical Fast 1-D DCT Algorithms with 11 Multiplications' paper

import math

def cos(x, u):
    return math.cos(((2*x + 1)*u*math.pi) / 16)

for x in range(0,8):
    for u in range(0, 8):
        print(cos(x,u))

#####

import math

N = 8
for k in range(0,N):
  out = []
  for n in range(0,N):
    v = math.cos((math.pi / N)*(n + (1/2)) * k) / 2
    if k == 0:
      v /= math.sqrt(2)
    out.append(v)
  print(out)



cos (pi / 8) * (n + 1/2) * k

    (pi * k) / 8

    n * pi * k / 8    + pi * k / 16
    (2 * n * pi * k + pi * k) / 16

    ((2 * n + 1) * (pi*k)) / 16

*/

const DCT_MAT_8X8: &[f32; BLOCK_SIZE] = &[
    0.35355339059327373,
    0.35355339059327373,
    0.35355339059327373,
    0.35355339059327373,
    0.35355339059327373,
    0.35355339059327373,
    0.35355339059327373,
    0.35355339059327373, //
    0.4903926402016152,
    0.4157348061512726,
    0.27778511650980114,
    0.09754516100806417,
    -0.0975451610080641,
    -0.277785116509801,
    -0.4157348061512727,
    -0.4903926402016152, //
    0.46193976625564337,
    0.19134171618254492,
    -0.19134171618254486,
    -0.46193976625564337,
    -0.4619397662556434,
    -0.19134171618254517,
    0.191341716182545,
    0.46193976625564326, //
    0.4157348061512726,
    -0.0975451610080641,
    -0.4903926402016152,
    -0.2777851165098011,
    0.2777851165098009,
    0.4903926402016153,
    0.09754516100806396,
    -0.4157348061512721, //
    0.3535533905932738,
    -0.35355339059327373,
    -0.35355339059327384,
    0.3535533905932737,
    0.35355339059327384,
    -0.35355339059327334,
    -0.35355339059327356,
    0.3535533905932733, //
    0.27778511650980114,
    -0.4903926402016152,
    0.09754516100806415,
    0.41573480615127273,
    -0.41573480615127256,
    -0.09754516100806489,
    0.49039264020161516,
    -0.27778511650980076, //
    0.19134171618254492,
    -0.4619397662556434,
    0.46193976625564326,
    -0.19134171618254495,
    -0.19134171618254528,
    0.4619397662556437,
    -0.46193976625564354,
    0.19134171618254314, //
    0.09754516100806417,
    -0.2777851165098011,
    0.41573480615127273,
    -0.4903926402016153,
    0.4903926402016152,
    -0.415734806151272,
    0.2777851165098022,
    -0.09754516100806254, //
];

fn mat_index(a: &[f32; BLOCK_SIZE], i: usize, j: usize) -> &f32 {
    &a[i * 8 + j]
}

fn mat_index_mut(a: &mut [f32; BLOCK_SIZE], i: usize, j: usize) -> &mut f32 {
    &mut a[i * 8 + j]
}

fn matmul(a: &[f32; BLOCK_SIZE], b: &[f32; BLOCK_SIZE], c: &mut [f32; BLOCK_SIZE]) {
    for i in 0..8 {
        for j in 0..8 {
            let c_ij = mat_index_mut(c, i, j);
            *c_ij = 0.0;
            for k in 0..8 {
                *c_ij += *mat_index(a, k, i) * *mat_index(b, k, j);
            }
        }
    }
}

fn matmul_tb(a: &[f32; BLOCK_SIZE], b: &[f32; BLOCK_SIZE], c: &mut [f32; BLOCK_SIZE]) {
    for i in 0..8 {
        for j in 0..8 {
            let c_ij = mat_index_mut(c, i, j);
            *c_ij = 0.0;
            for k in 0..8 {
                *c_ij += *mat_index(a, i, k) * *mat_index(b, k, j);
            }
        }
    }
}

fn inverse_dct_2d(input: &[i16; BLOCK_SIZE], output: &mut [i16; BLOCK_SIZE]) {
    let mut temp1 = [0f32; BLOCK_SIZE];
    for (i, v) in input.iter().enumerate() {
        temp1[i] = *v as f32;
    }

    let mut temp2 = [0f32; BLOCK_SIZE];

    // = M' * X * M
    matmul(DCT_MAT_8X8, &temp1, &mut temp2);
    matmul_tb(&temp2, DCT_MAT_8X8, &mut temp1);

    for (i, v) in temp1.iter().enumerate() {
        output[i] = v.round() as i16;
    }

    return;

    let alpha = |v: u8| -> f32 {
        if v == 0 {
            1.0f32 / (2.0f32).sqrt() as f32
        } else {
            1.0f32
        }
    };

    // TODO: Make into a LUT
    let cos = |x: u8, u: u8| -> f32 { (((2.0 * (x as f32) + 1.0) * (u as f32) * PI) / 16.0).cos() };

    for i in 0..(output.len() as u8) {
        let x = i % 8;
        let y = i / 8;

        let mut sum = 0.0;
        for v in 0..8_u8 {
            for u in 0..8_u8 {
                sum += alpha(u)
                    * alpha(v)
                    * (input[(v * 8 + u) as usize] as f32)
                    * cos(x, u)
                    * cos(y, v);
            }
        }

        // TODO: The 1/4 could be a >> 2 in integer space done at the very end?
        output[i as usize] = (((1.0 / 4.0) * sum) as f32).round() as i16;
    }
}

// TODO: Verify that it matches the precision of the frame.
fn dequantize(inputs: &mut [i16], table: &DefineQuantizationTable) {
    match &table.elements {
        DefineQuantizationTableElements::U8(vals) => {
            for (input, coeff) in inputs.iter_mut().zip(*vals) {
                *input *= *coeff as i16;
            }
        }
        DefineQuantizationTableElements::U16(vals) => {
            for (input, coeff) in inputs.iter_mut().zip(vals) {
                *input *= *coeff as i16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_zz_works() {
        assert_eq!(decode_zz(2, 0b00), -3);
        assert_eq!(decode_zz(2, 0b01), -2);
        assert_eq!(decode_zz(2, 0b10), 2);
        assert_eq!(decode_zz(2, 0b11), 3);
    }
}

// Based on T.871
// TODO: This is highly parallelizable (ideally do in CPU cache when decoding
// MCUs)
fn jpeg_ycbcr_to_rgb(inputs: &mut [u8]) {
    let clamp = |v: f32| -> u8 { v.round().max(0.0).min(255.0) as u8 };

    for tuple in inputs.chunks_mut(3) {
        let y = tuple[0] as f32;
        let cb = tuple[1] as f32;
        let cr = tuple[2] as f32;

        // TODO: Pre-subtract 128

        let r = y + 1.402 * (cr - 128.0);
        let g = y - 0.3441 * (cb - 128.0) - 0.7141 * (cr - 128.0);
        let b = y + 1.772 * (cb - 128.0);

        tuple[0] = clamp(r);
        tuple[1] = clamp(g);
        tuple[2] = clamp(b);
    }
}

const MAX_DC_TABLES: usize = 4;
const MAX_AC_TABLES: usize = 4;
const MAX_QUANT_TABLES: usize = 4;

pub struct JPEG {
    // TODO: May also be up to 12 bits of precision.
    pub image: Image<u8>,
}

/// For a single component of an image, contains currently accumulated data.
struct FrameComponentData {
    /// Index of this component in the pixel data ordering.
    index: usize,

    /// Width of this component's data.
    /// (includes padding and sub-sampling of the full image's width)
    x_i: usize,

    /// Height of this component's data.
    /// (includes padding and sub-sampling of the full image's height)
    y_i: usize,

    /// Total number of blocks required to represent this component.
    num_blocks: usize,

    /// Values of the DCT coefficients for all blocks in the image as of now.
    /// These are the raw values found after entropy decoding. They are in
    /// zig-zag order and still need to be de-quantized.
    ///
    /// TODO: We can optimize out storing this buffer if in sequential mode.
    raw_coeffs: Vec<[i16; BLOCK_SIZE]>,

    /// Tracks which of the coefficients have been seen in a scan so far.
    /// Starts at being all false before the first scan and is incrementally
    /// filled in as raw_coeffs is updated with new scans.
    seen_coeffs: [bool; BLOCK_SIZE],
}

fn parse_restart_marker(byte: u8) -> Option<u8> {
    if byte >= 0xd0 && byte <= 0xd7 {
        Some(byte - 0xd0)
    } else {
        None
    }
}

struct StuffedReader<'a, T: Read> {
    inner: &'a mut T,
}

impl<'a, T: Read> StuffedReader<'a, T> {
    fn new(inner: &'a mut T) -> Self {
        Self { inner }
    }
}

impl<'a, T: Read> Read for StuffedReader<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.len() != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Only reading one byte at a time is currently supported",
            ));
        }

        {
            let n = self.inner.read(buf)?;
            if n == 0 {
                return Ok(0);
            }
        }

        if buf[0] == 0xff {
            let mut temp = [0u8; 1];
            let n = self.inner.read(&mut temp)?;

            if n != 1 || temp[0] != 0x00 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Expected 0xFF to be stuffed by 0x00",
                ));
            }
        }

        Ok((1))
    }
}

// TODO: Validate that in progressive mode, DC scans always happen before AC.
// ^ Guranteed by G.1.1.1.1

// TODO: Go back through all the segments and verify that all values are in
// range.

impl JPEG {
    pub fn open(path: &str) -> Result<JPEG> {
        let mut file = File::open(path)?;

        // TODO: Limit max size of the jpeg

        let mut buf = vec![];
        file.read_to_end(&mut buf)?;

        Self::parse(&buf)
    }

    // TODO: It is still possible for this to crash if we perform multiplies that
    // overflow. Should just ignore these cases and warn.
    pub fn parse(data: &[u8]) -> Result<JPEG> {
        let mut next = data;

        // TODO: Verify B.2.4.4 "The SOI marker disables the restart interval"
        if parse_next!(next, take_exact(2)) != START_OF_IMAGE {
            return Err(err_msg("Invalid start bytes"));
        }

        // TODO: Be sure to pre-validate the range of all of the indices referencing
        // these.
        let mut dc_huffman_trees: [Option<HuffmanTree>; MAX_DC_TABLES] = [None, None, None, None];
        let mut ac_huffman_trees: [Option<HuffmanTree>; MAX_AC_TABLES] = [None, None, None, None];

        let mut quantization_tables: [Option<DefineQuantizationTable>; MAX_QUANT_TABLES] =
            [None, None, None, None];

        let mut frame_segment: Option<StartOfFrameSegment> = None;

        // TODO: Combine this with frame_components_data.
        let mut frame_components: HashMap<u8, FrameComponent> = HashMap::new();

        // TODO: Just base everything on indices.
        let mut frame_component_data: HashMap<u8, FrameComponentData> = HashMap::new();

        let mut h_max = 0;
        let mut v_max = 0;

        let mut restart_interval = None;

        // TODO: For each component, we should record how many of the components we have
        // seen so far.

        // RGB values of pixels.
        // TODO: Support non-3 channel modes
        let mut pixels = vec![];

        loop {
            let mut marker = parse_next!(next, take_exact(2));
            assert_eq!(marker[0], 0xff);

            if marker[1] == END_OF_IMAGE {
                // NOTE: Some valid images have extra data at the end.
                // assert_eq!(next.len(), 0);
                break;
            }

            let size = parse_next!(next, be_u16);
            assert!(size > 2); // Must at least contain the size itself.

            let mut inner = parse_next!(next, take_exact((size - 2) as usize));

            // TODO: Must error out if we see unknown markers that aren't APPx markers.

            match marker[1] {
                APP0 => {
                    let seg = App0Segment::parse(inner)?;
                    // println!("{:?}", seg);
                }
                DAC => {
                    return Err(err_msg("Arithmetic coding not supported"));
                }
                SOF0 | SOF1 | SOF2 | SOF3 => {
                    let seg = StartOfFrameSegment::parse(marker[1], inner)?;
                    // println!("{:?}", seg);

                    match seg.mode {
                        DCTMode::Baseline => {}
                        DCTMode::Progressive => {}
                        _ => {
                            return Err(format_err!("Unsupported DCT mode {:?}", seg.mode));
                        }
                    }

                    if frame_segment.is_some() {
                        return Err(err_msg("Received multiple frame headers"));
                    }

                    if seg.precision != 8 {
                        return Err(err_msg("Only 8-bit precision is currently supported"));
                    }

                    if seg.components.len() != 3 {
                        return Err(err_msg("Only JPEGs with 3 components are supported"));
                    }

                    pixels.resize(
                        (seg.y as usize) * (seg.x as usize) * seg.components.len(),
                        0,
                    );

                    for (i, component) in seg.components.iter().enumerate() {
                        if frame_components.contains_key(&component.id) {
                            return Err(err_msg("Duplicate component ids"));
                        }

                        frame_components.insert(component.id, component.clone());

                        // Sampling factors must be in the range [1, 4]
                        if component.v_factor < 1
                            || component.v_factor > 4
                            || component.h_factor < 1
                            || component.h_factor > 4
                        {
                            return Err(err_msg("Invalid component sampling factors"));
                        }

                        v_max = v_max.max(component.v_factor);
                        h_max = h_max.max(component.h_factor);
                    }

                    for (i, component) in seg.components.iter().enumerate() {
                        // TODO: Can we save space by assuming that ids will be < 4?

                        let v_i = component.v_factor;
                        let h_i = component.h_factor;

                        if h_max % h_i != 0 || v_max % v_i != 0 {
                            return Err(err_msg(
                                "Expected sampling factors to be exact multiples of each other",
                            ));
                        }

                        let x = seg.x as usize;
                        let y = seg.y as usize;

                        // Size in pixels of this component's 'frame'.
                        // A.1.1
                        let mut x_i = ceil_div(x * h_i, h_max);
                        let mut y_i = ceil_div(y * v_i, v_max);

                        // A.2.4
                        // First extend x_i and y_i to full block intervals.
                        // Then extend to an integer multiple of H_i / V_i
                        x_i = (BLOCK_DIM * h_i) * ceil_div(x_i, BLOCK_DIM * h_i);
                        y_i = (BLOCK_DIM * v_i) * ceil_div(y_i, BLOCK_DIM * v_i);

                        let num_blocks = (x_i / BLOCK_DIM) * (y_i / BLOCK_DIM);

                        frame_component_data.insert(
                            component.id,
                            FrameComponentData {
                                index: i,
                                num_blocks,
                                x_i,
                                y_i,
                                raw_coeffs: vec![[0i16; 64]; num_blocks],
                                seen_coeffs: [false; BLOCK_SIZE],
                            },
                        );
                    }

                    frame_segment = Some(seg);
                }
                DQT => {
                    while !inner.is_empty() {
                        let seg = parse_next!(inner, DefineQuantizationTable::parse);
                        let id = seg.table_dest_id;
                        quantization_tables[id] = Some(seg);
                    }

                    // TODO: Error out if replacing a quantization table as we
                    // currently assume that we are able to
                    // perform de-quantization at the very end after all scans
                    // are done.
                }
                DHT => {
                    while !inner.is_empty() {
                        let seg = parse_next!(inner, DefineHuffmanTableSegment::parse);
                        if seg.table_class == TableClass::DC {
                            dc_huffman_trees[seg.table_dest_id] = Some(seg.to_tree());
                        } else if seg.table_class == TableClass::AC {
                            ac_huffman_trees[seg.table_dest_id] = Some(seg.to_tree());
                        }
                    }
                }
                DRI => {
                    let v = parse_next!(inner, be_u16) as usize;
                    restart_interval = if v == 0 { None } else { Some(v) };
                    assert!(inner.is_empty());
                }
                START_OF_SCAN => {
                    let seg = StartOfScanSegment::parse(inner)?;
                    // println!("{:?}", seg);

                    // TODO: Make all of the error cases 'unlikely'
                    if seg.selection_end < seg.selection_start {
                        return Err(err_msg("Selection end before start"));
                    }
                    if seg.selection_end > (BLOCK_SIZE - 1) as u8 {
                        return Err(err_msg("Selection out of range"));
                    }

                    if let Some(frame_seg) = &frame_segment {
                        if frame_seg.mode == DCTMode::Baseline {
                            if seg.selection_start != 0 || seg.selection_end != 63 {
                                return Err(err_msg("Invalid selection indices for Baseline mode"));
                            }
                        }
                    } else {
                        return Err(err_msg("Expected SOF before SOS"));
                    }

                    // TODO: Verify next is not empty?
                    // I guess it is valid for an empty image?
                    let mut encoded = None;

                    // Find the end of the encoded data (skipping byte stuffing and restarts until
                    // we hit a real marker).
                    for i in 0..(next.len() - 1) {
                        if next[i] == 0xff
                            && next[i + 1] != 0x00
                            && parse_restart_marker(next[i + 1]).is_none()
                        {
                            encoded = Some(&next[0..i]);
                            next = &next[i..];
                            break;
                        }
                    }

                    // TODO: Support restarts

                    if seg.components.is_empty() {
                        return Err(err_msg("No components in SOS"));
                    }

                    // TODO: Convert all selectors to indexes in the array.

                    // TODO: Verify that no duplicate components are given. Verify that components
                    // are in the same order as in the frame list.

                    let num_mcus = if seg.components.len() == 1 {
                        frame_component_data
                            .get(&seg.components[0].component_selector)
                            .unwrap()
                            .num_blocks
                    } else {
                        let comp = frame_components
                            .get(&seg.components[0].component_selector)
                            .unwrap();

                        let fdata = frame_component_data
                            .get(&seg.components[0].component_selector)
                            .unwrap();
                        // TODO: Verify that this value is the same regardless of which component is
                        // used for the calcualation.
                        fdata.num_blocks / (comp.h_factor * comp.v_factor)
                    };

                    let mut next_restart_i = 0;

                    // ^ might as well also drop restart markers.
                    let mut cursor = Cursor::new(encoded.unwrap());

                    let mut mcu_start_i = 0;
                    let mcu_interval = restart_interval.unwrap_or(num_mcus);

                    while mcu_start_i < num_mcus {
                        if mcu_start_i > 0 {
                            // TODO: Should verify that we are padded with '1' bits?
                            // reader.align_to_byte();
                            // reader.consume();

                            let mut marker = [0u8; 2];
                            cursor.read(&mut marker)?;

                            if marker[0] != 0xff {
                                return Err(err_msg("Invalid restart marker"));
                            }

                            let restart_i = match parse_restart_marker(marker[1]) {
                                Some(i) => i,
                                None => {
                                    return Err(err_msg("Invalid restart marker (2)"));
                                }
                            };

                            if restart_i != next_restart_i {
                                return Err(err_msg("Out of order restart index"));
                            }

                            next_restart_i = (restart_i + 1) % 8;

                            // TODO: Implement true parallized coding across the
                            // restart
                            // interval. Also verify that restart markers are
                            // sequential and
                            // store handling them in the StuffedReader

                            // TODO: Double check what else must be cleared
                            // along restarts?

                            // assert_eq!(eobrun, 0);
                        }

                        let mcu_end_i = (mcu_start_i + mcu_interval).min(num_mcus);
                        Self::read_mcus(
                            &mut cursor,
                            mcu_start_i,
                            mcu_end_i,
                            &seg,
                            &frame_components,
                            &mut frame_component_data,
                            &quantization_tables,
                            &dc_huffman_trees,
                            &ac_huffman_trees,
                            frame_segment.as_ref().unwrap(),
                            h_max,
                            v_max,
                            &mut pixels,
                        )?;
                        mcu_start_i = mcu_end_i;
                    }

                    ////

                    // TODO: Verify that we are now aligned to a byte offset in the Bitreader
                    assert_eq!(cursor.position() as usize, encoded.unwrap().len());
                }
                _ => {
                    println!("Unknown marker: {:x?}", marker);
                    println!("size: {}", size);
                }
            };
        }

        let frame_seg = frame_segment.unwrap();

        jpeg_ycbcr_to_rgb(&mut pixels);

        let mut arr = Array::<u8> {
            shape: vec![frame_seg.y as usize, frame_seg.x as usize, 3],
            data: pixels,
        };

        Ok(JPEG {
            image: Image {
                array: arr,
                colorspace: Colorspace::RGB,
            },
        })
    }

    /// Reads and decodes an uninterrupted range of MCUs (not containing
    /// restarts).
    fn read_mcus(
        cursor: &mut Cursor<&[u8]>,
        mcu_start_i: usize,
        mcu_end_i: usize,
        seg: &StartOfScanSegment,
        frame_components: &HashMap<u8, FrameComponent>,
        frame_component_data: &mut HashMap<u8, FrameComponentData>,
        quantization_tables: &[Option<DefineQuantizationTable>; MAX_QUANT_TABLES],
        dc_huffman_trees: &[Option<HuffmanTree>; MAX_DC_TABLES],
        ac_huffman_trees: &[Option<HuffmanTree>; MAX_AC_TABLES],
        frame_segment: &StartOfFrameSegment,
        h_max: usize,
        v_max: usize,
        pixels: &mut [u8],
    ) -> Result<()> {
        let mut stuffed_reader = StuffedReader::new(cursor);

        let mut reader = BitReader::new_with_order(&mut stuffed_reader, BitOrder::MSBFirst);
        // TODO: Run consume() frequently?

        // Last decoded DC coefficient value per component.
        // TODO: This can easily become a flat vector based on component index.
        // (we can bound the size of it to 4 entries due to the )
        // Implement as a FixedArray<i16, MaxNumChannels>
        // let mut last_dc: HashMap<u8, i16> = HashMap::new();

        let mut last_dc = HashMap::new(); // [0i16; MAX_NUM_COMPONENTS];

        // NOTE: Only applies to scans only encoding AC coefficients.
        let mut eobrun: usize = 0;

        for mcu_i in mcu_start_i..mcu_end_i {
            for component in &seg.components {
                let frame_component = frame_components.get(&component.component_selector).unwrap();
                let qtable = quantization_tables
                    [frame_component.quantization_table_selector as usize]
                    .as_ref()
                    .unwrap();

                // TODO: Stop shadowing the name of the map variable.
                let frame_component_data = frame_component_data
                    .get_mut(&component.component_selector)
                    .unwrap();

                let num_units = if seg.components.len() == 1 {
                    1
                } else {
                    frame_component.h_factor * frame_component.v_factor
                };

                for unit_i in 0..num_units {
                    // The index of the block in the component's sub-frame
                    let block = if seg.components.len() == 1
                        || (frame_component.h_factor == 1 && frame_component.v_factor == 1)
                    {
                        mcu_i
                    } else {
                        // TODO: We need to improve this calculation.

                        let blocks_per_x = (frame_component_data.x_i / BLOCK_DIM);

                        let block_x = (mcu_i * frame_component.h_factor) % blocks_per_x;
                        let block_y = frame_component.v_factor
                            * ((mcu_i * frame_component.h_factor) / blocks_per_x);

                        let mut block = block_y * blocks_per_x + block_x;

                        block += blocks_per_x * (unit_i / frame_component.h_factor);
                        block += (unit_i % frame_component.h_factor);

                        block
                    };

                    let buffer = &mut frame_component_data.raw_coeffs[block];

                    Self::read_block(
                        block,
                        buffer,
                        &mut reader,
                        frame_segment,
                        &seg,
                        component,
                        &dc_huffman_trees,
                        &ac_huffman_trees,
                        &mut last_dc,
                        &mut eobrun,
                    )?;

                    // After every scan, we re-update the pixels with any new
                    // information.
                    Self::decode_block(
                        block,
                        qtable,
                        frame_segment,
                        frame_component,
                        frame_component_data,
                        h_max,
                        v_max,
                        pixels,
                    );
                }

                // EOB runs can't span multiple components. They are only defined in the spec
                // for progressive refinement of a single component at a time.
                if seg.components.len() > 1 {
                    assert_eq!(eobrun, 0);
                }
            }
        }

        assert_eq!(eobrun, 0);

        // TODO: Verify that all remaining bits are 1's

        Ok(())
    }

    fn read_block(
        block: usize,
        buffer: &mut [i16; BLOCK_SIZE],
        reader: &mut BitReader,
        frame_seg: &StartOfFrameSegment,
        seg: &StartOfScanSegment,
        component: &ScanComponent,
        dc_huffman_trees: &[Option<HuffmanTree>; MAX_DC_TABLES],
        ac_huffman_trees: &[Option<HuffmanTree>; MAX_AC_TABLES],
        last_dc: &mut HashMap<u8, i16>,
        eobrun: &mut usize,
    ) -> Result<()> {
        // TODO: Update seen_coeffs

        // TODO: We should know if we are in a non-first successive refinement round
        // based on the 'Ah' (!= 0) bit position.

        // Read DC coefficient.
        if seg.selection_start == 0 {
            if seg.approximation_last_bit == 0 {
                // First pass

                let dc_tree = dc_huffman_trees[component.dc_table_selector as usize]
                    .as_ref()
                    .ok_or_else(|| err_msg("Referenced undefined DC table"))?;

                let s = dc_tree.read_code(reader)?;

                let mut v = if s > 0 {
                    let amplitude = reader.read_bits_be(s as u8)?;
                    decode_zz(s as usize, amplitude as u16)
                } else {
                    0
                };

                if last_dc.contains_key(&component.component_selector) {
                    v += *last_dc.get(&component.component_selector).unwrap();
                }

                last_dc.insert(component.component_selector, v);

                buffer[0] = v * (1 << (seg.approximation_cur_bit as i16));
            } else {
                // TODO: Verify that this works right with negative numbers?
                // TODO: Support more than one bit of
                // refinement?
                buffer[0] |= reader.read_bits_be(1)? as i16;
            }

            reader.consume();
        }

        // TODO: Verify that this default value makes sense (especially with the
        // assertion below).
        let mut coeff_i: usize = (seg.selection_start as usize).max(1);

        // Read AC coefficients.
        if seg.selection_end >= 1 && *eobrun == 0 {
            let ac_tree = ac_huffman_trees[component.ac_table_selector as usize]
                .as_ref()
                .ok_or_else(|| err_msg("Referenced undefined AC table"))?;

            while coeff_i <= seg.selection_end as usize {
                let sym = ac_tree.read_code(reader)?;

                let mut r = sym >> 4;
                let s = sym & 0b1111;

                if s == 0 {
                    if r == 15 {
                        // In this case, we got a ZRL symbol.
                        // Should skip 15 zeros and write one zero value.
                    } else {
                        // When R == 0 and S == 0, then we are in the regular
                        // EOB mode.
                        if r != 0
                            && (frame_seg.mode != DCTMode::Progressive
                                || seg.selection_start == 0
                                || seg.components.len() > 1)
                        {
                            return Err(err_msg(
                                "EOBn modes only defined in progressive mode when single component AC coefficients are being encoded."));
                        }

                        *eobrun += 1 << r;
                        *eobrun += reader.read_bits_be(r as u8)?;

                        break;
                    }
                }

                // NOTE: For sequential refinement, this should always be -1 or
                // 1 (excluding the ZRL case)
                let value = if s > 0 {
                    let amplitude = reader.read_bits_be(s as u8)?;
                    decode_zz(s as usize, amplitude as u16)
                } else {
                    0
                };

                // Zero Run Length
                // In sequential mode (or the first pass of a component)
                // TODO: How do we know if it is a second pass?
                {
                    // TOOD: If we are currently on a non-zero thingy, then we
                    // should bump up by one.

                    loop {
                        // TODO: Check that this doesn't go out of range.
                        if buffer[coeff_i] != 0 {
                            let correction = reader.read_bits_exact(1)?;
                            if correction == 1 {
                                buffer[coeff_i] += if buffer[coeff_i] > 0 { 1 } else { -1 };
                            }
                        } else {
                            if r == 0 {
                                break;
                            }

                            r -= 1;
                        }

                        coeff_i += 1;

                        if coeff_i > seg.selection_end as usize {
                            return Err(err_msg("Hit end of zero run"));
                            //
                            // break;
                        }

                        // TODO: Stop if we are out of
                        // bounds and error out if
                        // we didn't consume all of 'r'
                    }
                }

                // TODO: Apply any necessary shift to the
                // TODO: Check that this doesn't go out of range.
                buffer[coeff_i] += value * (1 << (seg.approximation_cur_bit as i16));
                coeff_i += 1;

                reader.consume();
            }
        }

        if *eobrun > 0 {
            while coeff_i <= seg.selection_end as usize {
                // TODO: DEDUP all of this correction code.
                // Apply correction bits until we hit the end.
                if buffer[coeff_i] != 0 {
                    let correction = reader.read_bits_exact(1)?;
                    if correction == 1 {
                        buffer[coeff_i] += if buffer[coeff_i] > 0 { 1 } else { -1 };
                    }
                }

                coeff_i += 1;
            }

            *eobrun -= 1;
        }

        assert_eq!(coeff_i, (seg.selection_end + 1) as usize);

        Ok(())
    }

    fn decode_block(
        block: usize,
        qtable: &DefineQuantizationTable,
        frame_segment: &StartOfFrameSegment,
        frame_component: &FrameComponent,
        frame_component_data: &FrameComponentData,
        h_max: usize,
        v_max: usize,
        pixels: &mut [u8],
    ) {
        // TODO: No need to copy if this is sequential mode.
        let mut buffer = frame_component_data.raw_coeffs[block].clone();

        dequantize(&mut buffer, &qtable);

        let mut buffer_dezig = [0; BLOCK_SIZE];
        reverse_zigzag(&mut buffer, &mut buffer_dezig);

        let mut buffer_out = [0; BLOCK_SIZE];
        inverse_dct_2d(&buffer_dezig, &mut buffer_out);

        //        println!("{:?}", &buffer_out[..]);

        for v in buffer_out.iter_mut() {
            *v += 128;

            // TODO: verify that all the 'as u8' casts clamp at these boundaries before
            // casting.
            if *v < 0 {
                *v = 0;
            } else if *v > 255 {
                *v = 255;
            }
        }

        let y_limit = frame_segment.y as usize;
        let x_limit = frame_segment.x as usize;

        let blocks_per_line = frame_component_data.x_i / BLOCK_DIM;

        // TODO: Need to know h_max and v_max to know how many times to replicate stuff.

        // TODO: Generalize this for any image size
        for bi in 0..buffer_out.len() {
            // Coordinates within the component's sub-sampled frame.
            let y_c = BLOCK_DIM * (block / blocks_per_line) + (bi / BLOCK_DIM);
            let x_c = BLOCK_DIM * (block % blocks_per_line) + (bi % BLOCK_DIM);

            let v_steps = v_max / frame_component.v_factor;
            let h_steps = h_max / frame_component.h_factor;

            for y_si in 0..v_steps {
                for x_si in 0..h_steps {
                    let y = y_c * v_steps + y_si;
                    let x = x_c * h_steps + x_si;

                    // Ignore pixels that are in the padding space necessary to pad
                    // up to 8x8 blocks.
                    if y >= y_limit || x >= x_limit {
                        continue;
                    }

                    let nc = frame_segment.components.len();

                    let ii = (y * x_limit * nc) + x * nc;

                    pixels[ii + (frame_component_data.index as usize)] = buffer_out[bi] as u8;
                }
            }
        }
    }
}

#[derive(Debug)]
struct App0Segment<'a> {
    id: &'a [u8], // Always 5 bytes
    version: &'a [u8],
    density_units: u8,
    x_density: u16,
    y_density: u16,
    x_thumbnail: u8,
    y_thumbnail: u8,
    thumbnail_data: &'a [u8],
}

impl<'a> App0Segment<'a> {
    fn parse(mut data: &'a [u8]) -> Result<Self> {
        let id = parse_next!(data, take_exact(5));
        let version = parse_next!(data, take_exact(2));
        let density_units = parse_next!(data, be_u8);
        let x_density = parse_next!(data, be_u16);
        let y_density = parse_next!(data, be_u16);
        let x_thumbnail = parse_next!(data, be_u8);
        let y_thumbnail = parse_next!(data, be_u8);

        if data.len() % 3 != 0 {
            return Err(err_msg("Number of thumbnail bytes not divisible by 3"));
        }

        Ok(Self {
            id,
            version,
            density_units,
            x_density,
            y_density,
            x_thumbnail,
            y_thumbnail,
            thumbnail_data: data,
        })
    }
}

#[derive(Debug, Clone)]
struct FrameComponent {
    id: u8,
    /// Horizontal sampling factor (u8)
    h_factor: usize,
    /// Vertical sampling factor (u8)
    v_factor: usize,
    quantization_table_selector: u8,
}

// TODO: Consider eventually refactoring all image size related data types back
// to u16.
#[derive(Debug)]
struct StartOfFrameSegment {
    mode: DCTMode,
    precision: u8,
    /// Number of scan lines in the frame (aka the height of the image) (u16)
    y: usize, // Y
    /// Number of samples per scan line (aka the width of the image) (u16)
    x: usize, // X
    components: Vec<FrameComponent>,
}

impl StartOfFrameSegment {
    fn parse(marker: u8, mut data: &[u8]) -> Result<Self> {
        let mode = match marker {
            SOF0 => DCTMode::Baseline,
            SOF1 => DCTMode::Extended,
            SOF2 => DCTMode::Progressive,
            SOF3 => DCTMode::Lossless,
            _ => {
                return Err(err_msg("Unsupported SOF marker"));
            }
        };

        let precision = parse_next!(data, be_u8);
        let y = parse_next!(data, be_u16) as usize;
        let x = parse_next!(data, be_u16) as usize;

        let num_components = parse_next!(data, be_u8);
        let mut components = vec![];
        for i in 0..num_components {
            let id = parse_next!(data, be_u8);
            let factors = parse_next!(data, be_u8);
            let quantization_table_selector = parse_next!(data, be_u8);

            components.push(FrameComponent {
                id,
                h_factor: (factors >> 4) as usize,
                v_factor: (factors & 0b1111) as usize,
                quantization_table_selector,
            });
        }

        Ok(Self {
            mode,
            precision,
            y,
            x,
            components,
        })
    }
}

#[derive(Debug)]
struct StartOfScanSegment {
    components: Vec<ScanComponent>,
    selection_start: u8,
    // NOTE: Will be 63 in sequential (non-progressive mode)
    selection_end: u8,

    approximation_last_bit: u8,
    approximation_cur_bit: u8,
}

// So, I have huffman tables:
// - number of codes of length 1-16.

impl StartOfScanSegment {
    fn parse(mut data: &[u8]) -> Result<Self> {
        let num_components = parse_next!(data, be_u8);
        let mut components = vec![];
        for i in 0..num_components {
            let component_selector = parse_next!(data, be_u8);

            let t = parse_next!(data, be_u8);
            let dc_table_selector = t >> 4;
            let ac_table_selector = t & 0b1111;
            components.push(ScanComponent {
                component_selector,
                dc_table_selector,
                ac_table_selector,
            });
        }

        let selection_start = parse_next!(data, be_u8);
        let selection_end = parse_next!(data, be_u8);
        let a = parse_next!(data, be_u8);

        if !data.is_empty() {
            return Err(err_msg("Unexpected data after SOS"));
        }

        Ok(Self {
            components,
            selection_start,
            selection_end,
            approximation_last_bit: (a >> 4),
            approximation_cur_bit: (a & 0b1111),
        })
    }
}

#[derive(Debug)]
struct ScanComponent {
    component_selector: u8,
    dc_table_selector: u8,
    ac_table_selector: u8,
}

#[derive(Debug)]
struct DefineQuantizationTable<'a> {
    table_dest_id: usize, // 0-3
    elements: DefineQuantizationTableElements<'a>,
}

#[derive(Debug)]
enum DefineQuantizationTableElements<'a> {
    U8(&'a [u8]),
    U16(Vec<u16>),
}

impl<'a> DefineQuantizationTable<'a> {
    fn parse(mut data: &'a [u8]) -> Result<(Self, &'a [u8])> {
        let v = parse_next!(data, be_u8);

        let precision = (v >> 4);
        let table_dest_id = (v & 0b1111) as usize;

        let elements = if precision == 0 {
            DefineQuantizationTableElements::U8(parse_next!(data, take_exact(64)))
        } else if precision == 1 {
            let mut els = vec![];
            for i in 0..64 {
                els.push(parse_next!(data, be_u16));
            }

            DefineQuantizationTableElements::U16(els)
        } else {
            return Err(err_msg("Unknown precision"));
        };

        Ok((
            Self {
                table_dest_id,
                elements,
            },
            data,
        ))
    }
}

#[derive(Debug, PartialEq)]
enum TableClass {
    DC,
    AC,
}

#[derive(Debug)]
struct DefineHuffmanTableSegment<'a> {
    table_class: TableClass,
    table_dest_id: usize, // values 0-3 (in baseline, can only by 0-1)

    /// Number of codes which have length 'i' bits where 'i-1' is the index into
    /// this array from 0-15. Thus all codes have <= 16 bits.
    /// (BITS)
    length_counts: &'a [u8],

    /// Values encoded by the huffman tree in order of increasing code length.
    /// (HUFFVAL)
    values: &'a [u8],
}

impl<'a> DefineHuffmanTableSegment<'a> {
    // TODO: Make sure that all segments allow multiple in one?
    fn parse(mut data: &'a [u8]) -> Result<(Self, &'a [u8])> {
        let t = parse_next!(data, be_u8);

        let table_class = {
            let tc = t >> 4;
            if tc == 1 {
                TableClass::AC
            } else if tc == 0 {
                TableClass::DC
            } else {
                return Err(err_msg("Invalid table class"));
            }
        };

        let table_dest_id = (t & 0b1111) as usize;

        let length_counts = parse_next!(data, take_exact(16));

        let num_params = length_counts.iter().sum::<u8>() as usize;
        let values = parse_next!(data, take_exact(num_params));

        Ok((
            Self {
                table_class,
                table_dest_id,
                length_counts,
                values,
            },
            data,
        ))
    }

    // TODO: We need to aggresively limit the max number of nodes required to store
    // the huffman table (ideally by storing long sequences of bits in a single
    // node?)
    fn to_tree(&self) -> HuffmanTree {
        // Based on Annex C of T.81

        // Expanded list of the size of each code (HUFFSIZES)
        // TODO: Make this into an iterator/generator so that we don't have to store the
        // full list.
        let mut sizes: Vec<u8> = vec![];
        sizes.reserve(self.values.len());
        for i in 0..self.length_counts.len() {
            for j in 0..self.length_counts[i] {
                sizes.push((i as u8) + 1);
            }
        }

        // List of all codes (HUFFCODE)
        let mut codes: Vec<BitVector> = vec![];
        {
            let mut k = 0;
            let mut code: u16 = 0;
            let mut si = sizes[0];

            loop {
                loop {
                    // The 'si' most least significant bits make up the code. With the MSB of these
                    // representing the root of the tree.
                    codes.push(BitVector::from_lower_msb(code as usize, si));

                    code += 1;
                    k += 1;

                    if k == sizes.len() || sizes[k] != si {
                        break;
                    }
                }

                if k == sizes.len() {
                    break;
                }

                let size_step = sizes[k] - si;
                code = code << (size_step as u16);
                si += size_step;
            }
        }

        let mut tree = HuffmanTree::new();
        for i in 0..self.values.len() {
            // TODO: Optimize the tree to use u8 symbols.
            //            println!("{} => {:?}", self.values[i], codes[i]);

            tree.insert(self.values[i] as usize, codes[i].clone());
        }

        tree
    }
}
