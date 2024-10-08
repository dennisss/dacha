use std::collections::HashMap;
use std::ops::Index;
use std::ops::IndexMut;

use common::bits::BitOrder;
use common::bits::BitVector;
use common::bits::BitWrite;
use common::bits::BitWriter;
use common::ceil_div;
use common::errors::*;
use common::hash::FastHasherBuilder;
use compression::huffman::HuffmanTree;
use math::array::Array;

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
    // Raw quantization tables.
    lumin_quant_table: QuantizationTable,
    chroma_quant_table: QuantizationTable,

    // Pre-processed versions of the raw tables that must be adding as scaling during DCT
    // computation.
    lumin_quant_table_scaling: [f32; BLOCK_SIZE],
    chroma_quant_table_scaling: [f32; BLOCK_SIZE],
}

struct Atom {
    table_class: TableClass,
    table_index: u8,
    code: u8,
    value: BitVector,
}

struct CodeTables {
    codes: Vec<BitVector>,
}

// TODO: Only 162 of the AC codes will ever be used.
// TODO: We can store all codes with just 8 bits + a length specifier since the
// rest of the bits are always ones.
const NUM_TABLES_PER_CLASS: usize = 2;
const NUM_DC_CODES: usize = NUM_TABLES_PER_CLASS * 16;
const NUM_AC_CODES: usize = NUM_TABLES_PER_CLASS * 256;

impl CodeTables {
    fn new() -> Self {
        // (DC | AC) * (Table index 0 or 1) * (256 u8 values)
        Self {
            codes: vec![BitVector::new(); NUM_DC_CODES + NUM_AC_CODES],
        }
    }

    fn get_index(&self, index: (TableClass, u8, u8)) -> usize {
        let mut i = match index.0 {
            TableClass::DC => (index.1 as usize) * 16,
            TableClass::AC => NUM_DC_CODES + (index.1 as usize) * 256,
        };

        i += index.2 as usize;
        i
    }
}

impl Index<(TableClass, u8, u8)> for CodeTables {
    type Output = BitVector;

    fn index(&self, index: (TableClass, u8, u8)) -> &Self::Output {
        &self.codes[self.get_index(index)]
    }
}

impl IndexMut<(TableClass, u8, u8)> for CodeTables {
    fn index_mut(&mut self, index: (TableClass, u8, u8)) -> &mut Self::Output {
        let i = self.get_index(index);
        &mut self.codes[i]
    }
}

struct ImageBlockView {
    image: Image<u8>,
    channels: usize,
    x_blocks: usize,
    y_blocks: usize,
}

impl ImageBlockView {
    fn new(image: Image<u8>) -> Self {
        let x_blocks = ceil_div(image.width(), BLOCK_DIM);
        let y_blocks = ceil_div(image.height(), BLOCK_DIM);

        Self {
            channels: image.channels(),
            x_blocks,
            y_blocks,
            image,
        }
    }

    fn get_block(&self, component_i: usize, block_i: usize) -> [u8; BLOCK_SIZE] {
        let mut out = [0u8; BLOCK_SIZE];

        let x_start = (block_i % self.x_blocks) * BLOCK_DIM;
        let y_start = (block_i / self.x_blocks) * BLOCK_DIM;

        for y_rel in 0..BLOCK_DIM {
            let mut y = y_start + y_rel;

            if std::intrinsics::unlikely(y >= self.image.height()) {
                y = self.image.height() - 1;
            }

            for x_rel in 0..BLOCK_DIM {
                let mut x = x_start + x_rel;

                if std::intrinsics::unlikely(x >= self.image.width()) {
                    x = self.image.width() - 1;
                }

                let i = y * self.channels * self.image.width() + self.channels * x + component_i;
                out[y_rel * BLOCK_DIM + x_rel] = self.image.array.data[i];
            }
        }

        out
    }
}

impl JPEGEncoder {
    pub fn new(quality: usize) -> Self {
        let (lumin_quant_table, chroma_quant_table) = create_quantization_tables(quality);

        let lumin_quant_table_scaling = Self::quant_table_to_scales(&lumin_quant_table);
        let chroma_quant_table_scaling = Self::quant_table_to_scales(&chroma_quant_table);

        Self {
            lumin_quant_table,
            chroma_quant_table,
            lumin_quant_table_scaling,
            chroma_quant_table_scaling,
        }
    }

    fn quant_table_to_scales(table: &QuantizationTable) -> [f32; BLOCK_SIZE] {
        let mut q = [0f32; BLOCK_SIZE];
        for i in 0..table.0.len() {
            q[i] = 1.0 / (table.0[i] as f32);
        }

        let mut q2 = [0f32; BLOCK_SIZE];
        reverse_zigzag(&q, &mut q2);
        q2
    }

    // TODO: NEed to support direct YUV  input and input of 4:2:2 and 4:2:0 YUV
    // formats.
    //
    // TODO: Throw an error instead of crashing if the image is empty.
    #[inline(never)]
    pub fn encode(&self, image: &Image<u8>, out: &mut Vec<u8>) -> Result<()> {
        if image.colorspace != Colorspace::RGB && image.colorspace != Colorspace::Grayscale {
            return Err(err_msg("Only encoding RGB images is supported"));
        }

        out.extend_from_slice(START_OF_IMAGE);

        DefineQuantizationTable {
            table_dest_id: 0,
            elements: DefineQuantizationTableElements::U8(&self.lumin_quant_table.0),
        }
        .serialize(out);

        if image.channels() > 1 {
            DefineQuantizationTable {
                table_dest_id: 1,
                elements: DefineQuantizationTableElements::U8(&self.chroma_quant_table.0),
            }
            .serialize(out);
        }

        let mut pixels = image.array.data.clone();

        if image.colorspace == Colorspace::RGB {
            jpeg_rgb_to_ycbcr(&mut pixels);
        }

        // TODO: Actual colorspace is JPEG YUV if input was RGB.
        let blocks = ImageBlockView::new(Image {
            array: Array {
                shape: image.array.shape.clone(),
                data: pixels,
            },
            colorspace: image.colorspace,
        });

        let atoms = self.build_atoms(&blocks)?;

        let mut frame_components = vec![FrameComponent {
            id: 1,
            h_factor: 1,
            v_factor: 1,
            quantization_table_selector: 0,
        }];

        if image.channels() > 1 {
            frame_components.extend([
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
            ]);
        }

        let start_of_frame = StartOfFrameSegment {
            mode: DCTMode::Baseline,
            precision: 8,
            y: image.height(),
            x: image.width(),
            components: frame_components,
        };
        start_of_frame.serialize(out);

        // Calculate the huffman codes.
        let code_table = Self::build_dynamic_code_table(&atoms, image.channels(), out)?;

        let mut scan_components = vec![ScanComponent {
            component_index: 0,
            dc_table_selector: 0,
            ac_table_selector: 0,
        }];

        if image.channels() > 1 {
            scan_components.extend([
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
            ]);
        }

        StartOfScanSegment {
            components: scan_components,
            selection_start: 0,
            selection_end: 63,
            approximation_last_bit: 0,
            approximation_cur_bit: 0,
        }
        .serialize(&start_of_frame, out);

        Self::write_scan_atoms(&atoms, &code_table, out)?;

        out.push(0xFF);
        out.push(END_OF_IMAGE);

        Ok(())
    }

    fn split_to_blocks(blocks: &ImageBlockView) -> Vec<Vec<[u8; 64]>> {
        let mut blocks_per_component = vec![];
        let channels = blocks.image.channels();

        for i in 0..channels {
            blocks_per_component.push(vec![[0u8; BLOCK_SIZE]; blocks.x_blocks * blocks.y_blocks]);
        }

        // Split the image into per-component blocks.
        // TODO: Instead of doing this, directly pull blocks in the next loop?
        for y in 0..(blocks.y_blocks * BLOCK_DIM) {
            for x in 0..(blocks.x_blocks * BLOCK_DIM) {
                for c in 0..channels {
                    // Padded blocks will be filled by duplicating the right and bottom most pixels
                    // of the input image.
                    let in_y = y.min(blocks.image.height() - 1);
                    let in_x = x.min(blocks.image.width() - 1);

                    let block_i = (x / BLOCK_DIM) + (y / BLOCK_DIM) * blocks.x_blocks;
                    let block_x = x % BLOCK_DIM;
                    let block_y = y % BLOCK_DIM;

                    blocks_per_component[c][block_i][block_y * BLOCK_DIM + block_x] =
                        blocks.image.array.data
                            [channels * (in_y * blocks.image.width() + in_x) + c];
                }
            }
        }

        blocks_per_component
    }

    fn build_atoms(&self, blocks: &ImageBlockView) -> Result<Vec<Atom>> {
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

        let channels = blocks.image.channels();

        let mut atoms = vec![];

        let mut last_dc = [0i16; 3];

        // Iterate over every block (interleaving each component) to construct atoms.
        for block_i in 0..(blocks.x_blocks * blocks.y_blocks) {
            for component in 0..channels {
                let block = {
                    let original_block = blocks.get_block(component, block_i);

                    let output_scale = {
                        if component == 0 {
                            &self.lumin_quant_table_scaling
                        } else {
                            &self.chroma_quant_table_scaling
                        }
                    };

                    // TODO: Also do this type of fused quantization in the decoder.
                    let mut new_block = [0; BLOCK_SIZE];
                    forward_dct_2d(&original_block, output_scale, &mut new_block);

                    let mut new_block2 = [0; BLOCK_SIZE];
                    apply_zigzag(&new_block, &mut new_block2);

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

        Ok(atoms)
    }

    fn build_dynamic_code_table(
        atoms: &[Atom],
        num_components: usize,
        out: &mut Vec<u8>,
    ) -> Result<CodeTables> {
        let mut code_table = CodeTables::new();

        for (table_class, table_index) in &[
            (TableClass::DC, 0),
            (TableClass::AC, 0),
            (TableClass::DC, 1),
            (TableClass::AC, 1),
        ] {
            if num_components == 1 && *table_index == 1 {
                continue;
            }

            let mut symbols = vec![];
            symbols.reserve_exact(257);

            for atom in atoms {
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
                code_table[(*table_class, *table_index, *value)] = code;
            }
        }

        Ok(code_table)
    }

    fn write_scan_atoms(atoms: &[Atom], code_table: &CodeTables, out: &mut Vec<u8>) -> Result<()> {
        let mut stuffed_writer = StuffedWriter::new(out);
        let mut writer = BitWriter::new_with_order(&mut stuffed_writer, BitOrder::MSBFirst);

        for atom in atoms {
            let code = &code_table[(atom.table_class, atom.table_index, atom.code)];
            writer.write_bitvec(code)?;
            writer.write_bitvec(&atom.value)?;
        }

        writer.finish()?;

        // TODO: Pad with 1-bits.

        Ok(())
    }
}
