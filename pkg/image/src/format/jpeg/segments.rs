use common::bits::{BitOrder, BitReader, BitVector};
use common::errors::*;
use common::InRange;
use compression::huffman::HuffmanTree;
use parsing::binary::{be_u16, be_u8};
use parsing::take_exact;

use crate::format::jpeg::markers::*;

#[derive(Debug, PartialEq)]
pub enum DCTMode {
    Baseline,
    Extended,
    Progressive,
    Lossless,
}

#[derive(Debug)]
pub struct App0Segment<'a> {
    pub id: &'a [u8], // Always 5 bytes
    pub version: &'a [u8],
    pub density_units: u8,
    pub x_density: u16,
    pub y_density: u16,
    pub x_thumbnail: u8,
    pub y_thumbnail: u8,
    pub thumbnail_data: &'a [u8],
}

impl<'a> App0Segment<'a> {
    pub fn parse(mut data: &'a [u8]) -> Result<Self> {
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

// TODO: Consider eventually refactoring all image size related data types back
// to u16.
#[derive(Debug)]
pub struct StartOfFrameSegment {
    pub mode: DCTMode,
    pub precision: u8,
    /// Number of scan lines in the frame (aka the height of the image) (u16)
    pub y: usize, // Y
    /// Number of samples per scan line (aka the width of the image) (u16)
    pub x: usize, // X
    pub components: Vec<FrameComponent>,
}

#[derive(Debug, Clone)]
pub struct FrameComponent {
    pub id: u8,
    /// Horizontal sampling factor (u8)
    pub h_factor: usize,
    /// Vertical sampling factor (u8)
    pub v_factor: usize,
    pub quantization_table_selector: u8,
}

impl StartOfFrameSegment {
    pub fn parse(marker: u8, mut data: &[u8]) -> Result<Self> {
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
        for _ in 0..num_components {
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

    pub fn serialize(&self, out: &mut Vec<u8>) {
        let marker = match self.mode {
            DCTMode::Baseline => SOF0,
            DCTMode::Extended => SOF1,
            DCTMode::Progressive => SOF2,
            DCTMode::Lossless => SOF3,
        };

        serialize_segment(marker, out, |out| {
            out.push(self.precision);
            out.extend_from_slice(&(self.y as u16).to_be_bytes());
            out.extend_from_slice(&(self.x as u16).to_be_bytes());
            out.push(self.components.len() as u8);

            for component in &self.components {
                out.push(component.id);
                out.push(((component.h_factor as u8) << 4) | (component.v_factor as u8));
                out.push(component.quantization_table_selector);
            }
        });
    }
}

#[derive(Debug)]
pub struct StartOfScanSegment {
    pub components: Vec<ScanComponent>,

    /// Index of the first coefficient stored in this scan. (min is 0).
    pub selection_start: u8,

    /// Index of the last coefficient stored in this scan.
    /// NOTE: Will be 63 in sequential (non-progressive mode)
    pub selection_end: u8,

    pub approximation_last_bit: u8,
    pub approximation_cur_bit: u8,
}

#[derive(Debug)]
pub struct ScanComponent {
    /// The index of this component in the frame components list.
    /// NOTE: This is different than the 'C_sj' component selector which is
    /// stored in the scan header binary data.
    pub component_index: usize,
    pub dc_table_selector: u8,
    pub ac_table_selector: u8,
}

impl StartOfScanSegment {
    pub fn parse(frame_header: &StartOfFrameSegment, mut data: &[u8]) -> Result<Self> {
        let num_components = parse_next!(data, be_u8);
        let mut components = vec![];
        let mut next_component_index = 0;
        for _ in 0..num_components {
            let component_selector = parse_next!(data, be_u8);

            let component_index = {
                let mut idx = next_component_index;
                loop {
                    if idx >= frame_header.components.len() {
                        // This will be triggered if we couldn't find the selector in the frame
                        // header. This will also be triggered if the scan header references
                        // components in a different order compared to the
                        // frame header or if there are duplicates in the
                        // scan header. both of these cases are invalid
                        // according to the spec.
                        return Err(err_msg(
                            "Failed to find component referenced in scan header",
                        ));
                    }

                    if frame_header.components[idx].id == component_selector {
                        break;
                    }

                    idx += 1;
                }

                next_component_index = idx + 1;
                idx
            };

            let t = parse_next!(data, be_u8);
            let dc_table_selector = t >> 4;
            let ac_table_selector = t & 0b1111;

            if !(dc_table_selector.in_range(0, 3) || ac_table_selector.in_range(0, 3)) {
                return Err(err_msg("Out of range field values"));
            }

            components.push(ScanComponent {
                component_index,
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

    pub fn serialize(&self, start_of_frame: &StartOfFrameSegment, out: &mut Vec<u8>) {
        serialize_segment(START_OF_SCAN, out, |out| {
            out.push(self.components.len() as u8);

            for component in &self.components {
                out.push(start_of_frame.components[component.component_index].id);
                out.push((component.dc_table_selector << 4) | component.ac_table_selector);
            }

            out.push(self.selection_start);
            out.push(self.selection_end);
            out.push((self.approximation_last_bit << 4) | self.approximation_cur_bit);
        });
    }
}

#[derive(Debug)]
pub struct DefineQuantizationTable<'a> {
    pub table_dest_id: usize, // 0-3
    pub elements: DefineQuantizationTableElements<'a>,
}

#[derive(Debug)]
pub enum DefineQuantizationTableElements<'a> {
    U8(&'a [u8]), // TODO: Verify always has 64 elements.
    U16(Vec<u16>),
}

impl<'a> DefineQuantizationTable<'a> {
    pub fn parse(mut data: &'a [u8]) -> Result<(Self, &'a [u8])> {
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

    pub fn serialize(&self, out: &mut Vec<u8>) {
        serialize_segment(DQT, out, |out| {
            let precision = match &self.elements {
                DefineQuantizationTableElements::U8(_) => 0,
                DefineQuantizationTableElements::U16(_) => 1,
            };

            out.push(((precision as u8) << 4) | (self.table_dest_id as u8));
            match &self.elements {
                DefineQuantizationTableElements::U8(vals) => out.extend_from_slice(vals),
                DefineQuantizationTableElements::U16(vals) => {
                    for v in vals {
                        out.extend_from_slice(&v.to_be_bytes());
                    }
                }
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableClass {
    DC,
    AC,
}

#[derive(Debug)]
pub struct DefineHuffmanTableSegment<'a> {
    pub table_class: TableClass,
    pub table_dest_id: usize, // values 0-3 (in baseline, can only by 0-1)

    /// Number of codes which have length 'i+1' bits where 'i' is the index into
    /// this array from 0-15. Thus all codes have <= 16 bits.
    /// (BITS)
    ///
    /// NOTE: There is always exactly 16 elements in this array.
    pub length_counts: &'a [u8],

    /// Values encoded by the huffman tree in order of increasing code length.
    /// (HUFFVAL)
    pub values: &'a [u8],
}

impl<'a> DefineHuffmanTableSegment<'a> {
    // TODO: Make sure that all segments allow multiple in one?
    pub fn parse(mut data: &'a [u8]) -> Result<(Self, &'a [u8])> {
        /*
        TODO: Things to validate:
        - For DC, values must be <= 15
        - No code should be all 1's
        */

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

    pub fn serialize(&self, out: &mut Vec<u8>) {
        // TODO: Support serializing multiple tables into one segment.

        serialize_segment(DHT, out, |out| {
            out.push(
                (match self.table_class {
                    TableClass::DC => 0,
                    TableClass::AC => 1,
                } << 4)
                    | (self.table_dest_id as u8),
            );

            assert_eq!(self.length_counts.len(), 16);
            out.extend_from_slice(self.length_counts);
            out.extend_from_slice(self.values);
        });
    }

    pub fn create_codes(&self) -> Vec<BitVector> {
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

        codes
    }

    // TODO: We need to aggresively limit the max number of nodes required to store
    // the huffman table (ideally by storing long sequences of bits in a single
    // node?)
    pub fn to_tree(&self) -> HuffmanTree {
        let codes = self.create_codes();

        let mut tree = HuffmanTree::new();
        for i in 0..self.values.len() {
            // TODO: Optimize the tree to use u8 symbols.
            //            println!("{} => {:?}", self.values[i], codes[i]);

            tree.insert(self.values[i] as usize, codes[i].clone());
        }

        tree
    }
}

fn serialize_segment<F: Fn(&mut Vec<u8>)>(marker: u8, out: &mut Vec<u8>, f: F) {
    out.push(0xFF);
    out.push(marker);

    let size_offset = out.len();
    out.push(0);
    out.push(0);

    f(out);

    *array_mut_ref![out, size_offset, 2] = ((out.len() - size_offset) as u16).to_be_bytes();
}
