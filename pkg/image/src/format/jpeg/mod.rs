mod coefficient;
pub mod color;
mod constants;
mod dct;
pub mod encoder;
mod markers;
mod quantization;
mod segments;
mod stuffed;
mod zigzag;

use std::f32::consts::PI;
use std::io::{Cursor, Read};

use common::bits::{BitOrder, BitReader, BitVector};
use common::ceil_div;
use common::errors::*;
use compression::huffman::HuffmanTree;
use constants::*;
use dct::*;
use math::array::Array;
use math::matrix::Dimension;
use parsing::binary::{be_u16, be_u8};
use parsing::take_exact;
use segments::*;

use crate::format::jpeg::coefficient::*;
use crate::format::jpeg::color::*;
use crate::format::jpeg::markers::*;
use crate::format::jpeg::stuffed::*;
use crate::format::jpeg::zigzag::*;
use crate::{Colorspace, Image};

/*
References:
https://en.wikipedia.org/wiki/JPEG_File_Interchange_Format
https://www.w3.org/Graphics/JPEG/itu-t81.pdf
https://www.w3.org/Graphics/JPEG/jfif3.pdf

See here for more test images:
https://www.w3.org/MarkUp/Test/xhtml-print/20050519/tests/A_2_1-BF-01.htm
*/

// TODO: Verify that it matches the precision of the frame (8-bit images should
// use the 8-bit tables).
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

pub struct JPEG {
    // TODO: May also be up to 12 bits of precision.
    pub image: Image<u8>,

    pub unknown_segments: Vec<(u8, Vec<u8>)>,

    /// Extra data after the end of the image which wasn't parsed.
    pub trailer_data: Vec<u8>,
}

/// For a single component of an image, contains currently accumulated data.
struct FrameComponentData {
    /// Index of this component in the pixel data ordering.
    index: usize,

    /// Width of this component's data (in pixel units).
    /// (includes padding and sub-sampling of the full image's width)
    x_i: usize,

    /// Height of this component's data (in pixel units).
    /// (includes padding and sub-sampling of the full image's height)
    y_i: usize,

    x_i_interleaved: usize,

    y_i_interleaved: usize,

    /// Total number of blocks required to represent this component.
    ///
    /// NOTE: While this may seem redundant with raw_coeffs.len(), it is
    /// possible that raw_coeffs is empty if we are in sequential model.
    num_blocks: usize,

    num_blocks_interleaved: usize,

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

// TODO: Validate that in progressive mode, DC scans always happen before AC.
// ^ Guranteed by G.1.1.1.1

// TODO: Go back through all the segments and verify that all values are in
// range.

impl JPEG {
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

        // TODO: Just base everything on indices.
        let mut frame_component_data: Vec<FrameComponentData> = vec![];

        let mut h_max = 0;
        let mut v_max = 0;

        let mut restart_interval = None;

        // TODO: For each component, we should record how many of the components we have
        // seen so far.

        // RGB values of pixels.
        // TODO: Support non-3 channel modes
        let mut pixels = vec![];

        let mut unknown_segments = vec![];

        loop {
            let mut marker = parse_next!(next, take_exact(2));
            if marker[0] != 0xff {
                return Err(err_msg("Invalid JPEG marker start byte"));
            }

            if marker[1] == END_OF_IMAGE {
                // NOTE: Some valid images have extra data at the end.
                // assert_eq!(next.len(), 0);
                break;
            }

            let size = parse_next!(next, be_u16);
            assert!(size > 2); // Must at least contain the size itself.

            let mut inner = parse_next!(next, take_exact((size - 2) as usize));

            // TODO: Must error out if we see unknown markers that aren't APPx markers.

            // let mut app0 = None;

            match marker[1] {
                APP0 => {
                    let app0 = Some(App0Segment::parse(inner)?);
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

                    let mut frame_components_idx = std::collections::HashSet::new();

                    for (i, component) in seg.components.iter().enumerate() {
                        if !frame_components_idx.insert(component.id) {
                            return Err(err_msg("Duplicate component ids"));
                        }

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

                        /*
                                                NOTE – It is recommended that any incomplete MCUs be completed by replication of the right-most column and the bottom
                        line of each component.
                                                */

                        let x = seg.x as usize;
                        let y = seg.y as usize;

                        // Size in pixels of this component's 'frame'.
                        // A.1.1
                        let mut x_i = ceil_div(x * h_i, h_max);
                        let mut y_i = ceil_div(y * v_i, v_max);

                        // A.2.4
                        // First extend x_i and y_i to full block intervals.
                        x_i = BLOCK_DIM * ceil_div(x_i, BLOCK_DIM);
                        y_i = BLOCK_DIM * ceil_div(y_i, BLOCK_DIM);

                        // A.2.4
                        // When interleaving with other components, we must extend the frame to an
                        // integer multiple of H_i and V_i.
                        let x_i_interleaved = (BLOCK_DIM * h_i) * ceil_div(x_i, BLOCK_DIM * h_i);
                        let y_i_interleaved = (BLOCK_DIM * v_i) * ceil_div(y_i, BLOCK_DIM * v_i);

                        let num_blocks = (x_i / BLOCK_DIM) * (y_i / BLOCK_DIM);

                        //
                        let num_blocks_interleaved =
                            (x_i_interleaved / BLOCK_DIM) * (y_i_interleaved / BLOCK_DIM);

                        frame_component_data.push(FrameComponentData {
                            index: i,
                            num_blocks,
                            num_blocks_interleaved,
                            x_i,
                            x_i_interleaved,
                            y_i,
                            y_i_interleaved,
                            raw_coeffs: vec![[0i16; 64]; num_blocks_interleaved],
                            seen_coeffs: [false; BLOCK_SIZE],
                        });
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
                    let frame_segment = frame_segment
                        .as_ref()
                        .ok_or_else(|| err_msg("Expected SOF before SOS"))?;

                    let seg = StartOfScanSegment::parse(frame_segment, inner)?;

                    // TODO: Make all of the error cases 'unlikely'
                    if seg.selection_end < seg.selection_start {
                        return Err(err_msg("Selection end before start"));
                    }
                    if seg.selection_end > (BLOCK_SIZE - 1) as u8 {
                        return Err(err_msg("Selection out of range"));
                    }

                    // NOTE: We don't enforce this as sometimes images lie about their mode.
                    // if frame_segment.mode == DCTMode::Baseline {
                    //     if seg.selection_start != 0 || seg.selection_end != 63 {
                    //         println!("{:?}", seg);

                    //         return Err(err_msg("Invalid selection indices for Baseline mode"));
                    //     }
                    // }

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

                    // Hit the end of the file before the next marker (likely this file is truncated
                    // or missing an END_OF_IMAGE marker). Assume that all the
                    // remaining data is scan data.
                    if encoded.is_none() {
                        encoded = Some(next);
                        next = &[];
                    }

                    // TODO: Support restarts

                    if seg.components.is_empty() {
                        return Err(err_msg("No components in SOS"));
                    }

                    let num_mcus = if seg.components.len() == 1 {
                        frame_component_data[seg.components[0].component_index].num_blocks
                    } else {
                        let comp = &frame_segment.components[seg.components[0].component_index];

                        let fdata = &frame_component_data[seg.components[0].component_index];
                        // TODO: Verify that this value is the same regardless of which component is
                        // used for the calcualation.
                        fdata.num_blocks_interleaved / (comp.h_factor * comp.v_factor)
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
                            &mut frame_component_data,
                            &quantization_tables,
                            &dc_huffman_trees,
                            &ac_huffman_trees,
                            frame_segment,
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
                    let is_app_segment = marker[1] >= 0xE0 && marker[1] <= 0xEF;
                    let is_comment = marker[1] == 0xFE;

                    // We need to be conservative about what we skip to avoid skipping data that
                    // might be critical to correctly decoding the image.
                    let safe_to_ignore = is_app_segment || is_comment;

                    if !safe_to_ignore {
                        return Err(format_err!(
                            "Unknown critical JPEG segment with marker: {:x?}",
                            marker
                        ));
                    }

                    unknown_segments.push((marker[1], inner.to_vec()));
                    // println!("Unknown marker: {:x?}", marker);
                    // println!("size: {}", size);
                }
            };
        }

        // TODO: We should be able to verify that we saw all components and all
        // coefficients and bits of each coefficients across all scans (without
        // duplicates)

        let frame_seg = frame_segment.unwrap();

        jpeg_ycbcr_to_rgb(&mut pixels);

        let mut arr = Array::<u8> {
            shape: vec![frame_seg.y as usize, frame_seg.x as usize, 3],
            data: pixels,
        };

        Ok(JPEG {
            // app0,
            image: Image {
                array: arr,
                colorspace: Colorspace::RGB,
            },
            unknown_segments,
            trailer_data: next.to_vec(),
        })
    }

    /// Reads and decodes an uninterrupted range of MCUs (not containing
    /// restarts).
    fn read_mcus(
        cursor: &mut Cursor<&[u8]>,
        mcu_start_i: usize,
        mcu_end_i: usize,
        seg: &StartOfScanSegment,
        frame_component_data: &mut [FrameComponentData],
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

        let mut last_dc = [0i16; MAX_NUM_COMPONENTS];

        // NOTE: Only applies to scans only encoding AC coefficients.
        let mut eobrun: usize = 0;

        for mcu_i in mcu_start_i..mcu_end_i {
            for component in &seg.components {
                let frame_component = &frame_segment.components[component.component_index];
                let qtable = quantization_tables
                    [frame_component.quantization_table_selector as usize]
                    .as_ref()
                    .unwrap();

                // TODO: Stop shadowing the name of the map variable.
                let frame_component_data = &mut frame_component_data[component.component_index];

                let num_units = if seg.components.len() == 1 {
                    1
                } else {
                    frame_component.h_factor * frame_component.v_factor
                };

                for unit_i in 0..num_units {
                    // The index of the block in the component's sub-frame
                    let block = {
                        if frame_component.h_factor == 1 && frame_component.v_factor == 1 {
                            mcu_i
                        } else if seg.components.len() == 1 {
                            let blocks_per_x = (frame_component_data.x_i / BLOCK_DIM);

                            let block_x = mcu_i % blocks_per_x;
                            let block_y = mcu_i / blocks_per_x;

                            let blocks_per_x_interleaved =
                                (frame_component_data.x_i_interleaved / BLOCK_DIM);

                            block_y * blocks_per_x_interleaved + block_x
                        } else {
                            // TODO: We need to improve this calculation.

                            let blocks_per_x = (frame_component_data.x_i_interleaved / BLOCK_DIM);

                            let block_x = (mcu_i * frame_component.h_factor) % blocks_per_x;
                            let block_y = frame_component.v_factor
                                * ((mcu_i * frame_component.h_factor) / blocks_per_x);

                            let mut block = block_y * blocks_per_x + block_x;

                            block += blocks_per_x * (unit_i / frame_component.h_factor);
                            block += (unit_i % frame_component.h_factor);

                            block
                        }
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
                    //
                    // TODO: Only do this at the very end after all scans are seen.
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

        // TODO: Verify that all remaining bits are 1's (and not too many extra bytes
        // are present)

        Ok(())
    }

    // eobrun is the number of blocks filled with nothing but zero coefficients.
    fn read_block(
        block: usize,
        buffer: &mut [i16; BLOCK_SIZE],
        reader: &mut BitReader,
        frame_seg: &StartOfFrameSegment,
        seg: &StartOfScanSegment,
        component: &ScanComponent,
        dc_huffman_trees: &[Option<HuffmanTree>; MAX_DC_TABLES],
        ac_huffman_trees: &[Option<HuffmanTree>; MAX_AC_TABLES],
        last_dc: &mut [i16; MAX_NUM_COMPONENTS],
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

                v += last_dc[component.component_index];
                last_dc[component.component_index] = v;

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

                /*
                TODO for when making the encoder: The Huffman codes for the 8-bit composite values are generated in such a way that no code
                consists entirely of 1-bits
                */

                // Number of zero coefficients between the last coefficient and the current one.
                let mut r = sym >> 4;

                // Basically this is the number of
                let s = sym & 0b1111;

                if s == 0 {
                    if r == 15 {
                        // In this case, we got a ZRL symbol.
                        // Should skip 15 zeros and write one zero value.
                    } else {
                        // When R == 0 and S == 0, then we are in the regular
                        // EOB mode.
                        if r != 0
                            && (/*frame_seg.mode != DCTMode::Progressive
                            || */seg.selection_start == 0 || seg.components.len() > 1)
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

                /*
                Quote from spec: "
                In addition whenever zero runs are coded with ZRL or EOBn codes, correction bits for those coefficients with non-zero
                history contained within the zero run are appended according to rule b above
                "
                */

                /*
                See G.1.2.3

                The four most significant bits, RRRR, give the number of zero coefficients that are between the current coefficient and the
                previously coded coefficient (or the start of band). Coefficients with non-zero history (a non-zero value coded in a
                previous scan) are skipped over when counting the zero coefficients.
                */

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

        let blocks_per_line = frame_component_data.x_i_interleaved / BLOCK_DIM;

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
