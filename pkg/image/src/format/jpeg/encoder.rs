use std::collections::HashMap;

use common::bits::BitOrder;
use common::bits::BitVector;
use common::bits::BitWrite;
use common::bits::BitWriter;
use common::ceil_div;
use common::errors::*;
use compression::huffman::HuffmanTree;

use crate::format::jpeg::coefficient::*;
use crate::format::jpeg::color::*;
use crate::format::jpeg::constants::*;
use crate::format::jpeg::dct::*;
use crate::format::jpeg::markers::*;
use crate::format::jpeg::quantization::*;
use crate::format::jpeg::segments::*;
use crate::format::jpeg::stuffed::*;
use crate::format::jpeg::zigzag::*;
use crate::Colorspace;
use crate::Image;

/// Creator of JPEG images.
///
/// A single instance represents a set of compression/encoding parameters and
/// can be used to encode multiple JPEG images.
pub struct JPEGEncoder {
    lumin_quant_table: [u8; BLOCK_SIZE],
    chroma_quant_table: [u8; BLOCK_SIZE],
}

struct Atom {
    table_class: TableClass,
    table_index: u8,
    code: u8,
    value: BitVector,
}

impl JPEGEncoder {
    pub fn new(quality: usize) -> Self {
        let (lumin_quant_table, chroma_quant_table) = create_quantization_tables(quality);
        Self {
            lumin_quant_table,
            chroma_quant_table,
        }
    }

    pub fn encode(&self, image: &Image<u8>, out: &mut Vec<u8>) -> Result<()> {
        if image.colorspace != Colorspace::RGB {
            return Err(err_msg("Only encoding RGB images is supported"));
        }

        out.extend_from_slice(START_OF_IMAGE);

        DefineQuantizationTable {
            table_dest_id: 0,
            elements: DefineQuantizationTableElements::U8(&self.lumin_quant_table),
        }
        .serialize(out);

        DefineQuantizationTable {
            table_dest_id: 1,
            elements: DefineQuantizationTableElements::U8(&self.chroma_quant_table),
        }
        .serialize(out);

        let mut pixels = image.array.data.clone();
        jpeg_rgb_to_ycbcr(&mut pixels);

        let x_blocks = ceil_div(image.width(), BLOCK_DIM);
        let y_blocks = ceil_div(image.height(), BLOCK_DIM);

        let mut blocks_per_component = vec![];
        for i in 0..3 {
            blocks_per_component.push(vec![[0i16; BLOCK_SIZE]; x_blocks * y_blocks]);
        }

        // Split the image into per-component blocks.
        // TODO: Instead of doing this, directly pull blocks in the next loop?
        for y in 0..(y_blocks * BLOCK_DIM) {
            for x in 0..(x_blocks * BLOCK_DIM) {
                for c in 0..3 {
                    // Padded blocks will be filled by duplicating the right and bottom most pixels
                    // of the input image.
                    let in_y = y.min(image.height() - 1);
                    let in_x = x.min(image.width() - 1);

                    let block_i = (x / BLOCK_DIM) + (y / BLOCK_DIM) * x_blocks;
                    let block_x = x % BLOCK_DIM;
                    let block_y = y % BLOCK_DIM;

                    blocks_per_component[c][block_i][block_y * BLOCK_DIM + block_x] =
                        pixels[3 * (in_y * image.width() + in_x) + c] as i16;
                }
            }
        }

        /*
        For DC coeff:
        - Code is S which is number of bits to encode amplitude
            - Amplitude is delta since last d  (means not parallelizable without restarts)

        For AC coeffs:
        - Code is RRRRSSSS
            - RRRR is number of zeros since last coeff (up to 15)
            - SSSS is number of bits for the amplitude of the coefficient.
            - We can use S=0, R=0 to store an EOB event (all coeffs until the end of the block are zeros).
        */

        let mut atoms = vec![];

        let mut last_dc = [0i16; 3];

        // Iterate over every block (interleaving each component) to construct atoms.
        for block_i in 0..(x_blocks * y_blocks) {
            for component in 0..blocks_per_component.len() {
                let block = {
                    let original_block = &mut blocks_per_component[component][block_i];

                    // Center around 0.
                    for v in original_block.iter_mut() {
                        *v -= 128;
                    }

                    let mut new_block = [0; BLOCK_SIZE];
                    forward_dct_2d(original_block, &mut new_block);

                    let mut new_block2 = [0; BLOCK_SIZE];
                    apply_zigzag(&new_block, &mut new_block2);

                    quantize_block(
                        if component == 0 {
                            &self.lumin_quant_table
                        } else {
                            &self.chroma_quant_table
                        },
                        &mut new_block2,
                    );

                    new_block2
                };

                // Encode DC coefficient.
                {
                    let diff = block[0] - last_dc[component];
                    last_dc[component] = block[0];

                    let (size, diff_value) = encode_zz(diff);
                    atoms.push(Atom {
                        table_class: TableClass::DC,
                        table_index: if component == 0 { 0 } else { 1 },
                        code: size as u8,
                        value: BitVector::from_lower_msb(diff_value as usize, size as u8),
                    });
                }

                // Encode AC coefficients.
                let mut coeff_i = 1;
                while coeff_i < block.len() {
                    let mut zero_run_length = 0;
                    while coeff_i < block.len() && block[coeff_i] == 0 && zero_run_length < 15 {
                        zero_run_length += 1;
                        coeff_i += 1;
                    }

                    if coeff_i == block.len() {
                        // EOB run
                        atoms.push(Atom {
                            table_class: TableClass::AC,
                            table_index: if component == 0 { 0 } else { 1 },
                            code: 0b00000000,
                            value: BitVector::new(),
                        });
                        break;
                    }

                    let (coeff_size, coeff_value) = encode_zz(block[coeff_i]);
                    atoms.push(Atom {
                        table_class: TableClass::AC,
                        table_index: if component == 0 { 0 } else { 1 },
                        code: ((zero_run_length as u8) << 4) | (coeff_size as u8),
                        value: BitVector::from_lower_msb(coeff_value as usize, coeff_size as u8),
                    });

                    coeff_i += 1;
                }
            }
        }

        let start_of_frame = StartOfFrameSegment {
            mode: DCTMode::Baseline,
            precision: 8,
            y: image.height(),
            x: image.width(),
            components: vec![
                FrameComponent {
                    id: 1,
                    h_factor: 1,
                    v_factor: 1,
                    quantization_table_selector: 0,
                },
                FrameComponent {
                    id: 2,
                    h_factor: 1,
                    v_factor: 1,
                    quantization_table_selector: 1,
                },
                FrameComponent {
                    id: 3,
                    h_factor: 1,
                    v_factor: 1,
                    quantization_table_selector: 1,
                },
            ],
        };
        start_of_frame.serialize(out);

        // Calculate the huffman codes.

        let mut code_table = HashMap::new();

        for (table_class, table_index) in &[
            (TableClass::DC, 0),
            (TableClass::AC, 0),
            (TableClass::DC, 1),
            (TableClass::AC, 1),
        ] {
            let mut symbols = vec![];
            for atom in &atoms {
                if atom.table_class == *table_class && atom.table_index == *table_index {
                    symbols.push(atom.code as usize);
                }
            }

            // Reserve the 256 symbol (which is never encoded because it is larger than one
            // byte) to prevent creating a code of all 1s.
            symbols.push(256);

            let mut symbols = HuffmanTree::build_length_limited_tree(&symbols, 16)?;
            symbols.sort_by(|a, b| a.length.cmp(&b.length));

            let mut huffman_length_counts = [0u8; 16];
            let mut huffman_values = vec![];

            for symbol in symbols {
                // Skip the placeholder symbol.
                // NOTE: build_length_limited_tree prioritized symbols of smaller symbol value
                // first so this should always be the longest code.
                if symbol.symbol == 256 {
                    continue;
                }

                let length_idx = symbol.length - 1;
                huffman_length_counts[length_idx] += 1;

                huffman_values.push(symbol.symbol as u8);
            }

            let segment = DefineHuffmanTableSegment {
                table_class: *table_class,
                table_dest_id: *table_index as usize,
                length_counts: &huffman_length_counts,
                values: &huffman_values,
            };

            segment.serialize(out);

            for (code, value) in segment.create_codes().into_iter().zip(segment.values) {
                code_table.insert((*table_class, *table_index, *value), code);
            }
        }

        StartOfScanSegment {
            components: vec![
                ScanComponent {
                    component_index: 0,
                    dc_table_selector: 0,
                    ac_table_selector: 0,
                },
                ScanComponent {
                    component_index: 1,
                    dc_table_selector: 1,
                    ac_table_selector: 1,
                },
                ScanComponent {
                    component_index: 2,
                    dc_table_selector: 1,
                    ac_table_selector: 1,
                },
            ],
            selection_start: 0,
            selection_end: 63,
            approximation_last_bit: 0,
            approximation_cur_bit: 0,
        }
        .serialize(&start_of_frame, out);

        let mut stuffed_writer = StuffedWriter::new(out);
        let mut writer = BitWriter::new_with_order(&mut stuffed_writer, BitOrder::MSBFirst);

        for atom in atoms {
            let code = code_table
                .get(&(atom.table_class, atom.table_index, atom.code))
                .unwrap();
            writer.write_bitvec(code)?;
            writer.write_bitvec(&atom.value)?;
        }

        writer.finish()?;

        drop(stuffed_writer);

        // TODO: Pad with 1-bits.

        out.push(0xFF);
        out.push(END_OF_IMAGE);

        Ok(())
    }
}
