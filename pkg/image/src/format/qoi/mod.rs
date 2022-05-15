use common::errors::*;
use parsing::binary::{be_u32, be_u8};
use parsing::{tag, take_exact};

use crate::{Array, Colorspace, Image};

const MAGIC: &[u8] = b"qoif";
const HASH_TABLE_SIZE: usize = 64;

type Pixel = [u8; 4];

fn hash_pixel(pixel: &Pixel) -> usize {
    ((pixel[0] as usize) * 3
        + (pixel[1] as usize) * 5
        + (pixel[2] as usize) * 7
        + (pixel[3] as usize) * 11)
        % HASH_TABLE_SIZE
}

pub struct QOIDecoder {}

impl QOIDecoder {
    pub fn new() -> Self {
        Self {}
    }

    pub fn decode(&self, mut input: &[u8]) -> Result<Image<u8>> {
        parse_next!(input, tag(MAGIC));

        let width = parse_next!(input, be_u32) as usize;
        let height = parse_next!(input, be_u32) as usize;
        // 3 = RGB, 4 = RGBA
        let channels = parse_next!(input, be_u8) as usize;
        // 0 = sRGB with linear alpha
        // 1 = all channels linear
        let colorspace = parse_next!(input, be_u8);

        if channels != 3 {
            return Err(err_msg("Only RGB supported"));
        }

        let mut last_pixel = [0, 0, 0, 255u8];
        let mut hash_table = [[0u8; 4]; HASH_TABLE_SIZE];

        let mut pixels = vec![];
        pixels.resize(width * height * channels, 0);

        let mut current_pixel_index = 0;

        while current_pixel_index < pixels.len() {
            let first_byte = parse_next!(input, be_u8);

            let top_bits = first_byte >> 6; // top 2 bits;

            let mut run_length = 1;

            if top_bits == 0b00 {
                // QOI_OP_INDEX
                let index = (first_byte & 0b111111) as usize;
                last_pixel = hash_table[index];
            } else if top_bits == 0b01 {
                // QOI_OP_DIFF

                let dr = ((first_byte >> 4) & 0b11).wrapping_sub(2);
                let dg = ((first_byte >> 2) & 0b11).wrapping_sub(2);
                let db = ((first_byte >> 0) & 0b11).wrapping_sub(2);

                last_pixel[0] = last_pixel[0].wrapping_add(dr);
                last_pixel[1] = last_pixel[1].wrapping_add(dg);
                last_pixel[2] = last_pixel[2].wrapping_add(db);
            } else if top_bits == 0b10 {
                // QOI_OP_LUMA

                let second_byte = parse_next!(input, be_u8);

                let dg = (first_byte & 0b111111).wrapping_sub(32);

                let dr_dg = ((second_byte >> 4) & 0b1111).wrapping_sub(8);
                let db_dg = ((second_byte >> 0) & 0b1111).wrapping_sub(8);

                let dr = dr_dg.wrapping_add(dg);
                let db = db_dg.wrapping_add(dg);

                last_pixel[0] = last_pixel[0].wrapping_add(dr);
                last_pixel[1] = last_pixel[1].wrapping_add(dg);
                last_pixel[2] = last_pixel[2].wrapping_add(db);
            } else if first_byte == 0b11111110 {
                // QOI_OP_RGB
                let color = parse_next!(input, take_exact(3));
                last_pixel[0..3].copy_from_slice(color);
            } else if first_byte == 0b11111111 {
                // QOI_OP_RGBA
                let color = parse_next!(input, take_exact(4));
                last_pixel.copy_from_slice(color);
            } else {
                // QOI_OP_RUN
                run_length += (first_byte & 0b111111) as usize;
            }

            hash_table[hash_pixel(&last_pixel)] = last_pixel;

            for i in 0..run_length {
                pixels[current_pixel_index..(current_pixel_index + channels)]
                    .copy_from_slice(&last_pixel[0..channels]);
                current_pixel_index += channels;
            }
        }

        // Usually [0, 0, 0, 0, 0, 0, 0, 1]
        if input.len() != 8 {
            return Err(err_msg("Missing end marker"));
        }

        Ok(Image {
            array: Array {
                shape: vec![height, width, channels],
                data: pixels,
            },
            colorspace: Colorspace::RGB,
        })
    }
}

pub struct QOIEncoder {}

impl QOIEncoder {
    pub fn new() -> Self {
        Self {}
    }

    // TODO: Support encoding to a cord?
    pub fn encode(&self, image: &Image<u8>, out: &mut Vec<u8>) -> Result<()> {
        if image.colorspace != Colorspace::RGB {
            return Err(err_msg("Only RGB is supported"));
        }

        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(image.width() as u32).to_be_bytes());
        out.extend_from_slice(&(image.height() as u32).to_be_bytes());
        out.push(3);
        out.push(1);

        let pixels = &image.array.data[..];

        let mut current_pixel_index = 0;

        let mut last_pixel = [0, 0, 0, 255u8];
        let mut hash_table = [[0u8; 4]; 64];

        let mut run_length = 0;

        while current_pixel_index < pixels.len() {
            let mut current_pixel = [0u8; 4];
            current_pixel[0..3]
                .copy_from_slice(&pixels[current_pixel_index..(current_pixel_index + 3)]);
            current_pixel[3] = 255;
            current_pixel_index += 3;

            if run_length < 62 && current_pixel == last_pixel {
                run_length += 1;
                continue;
            }

            // Try encode QOI_OP_RUN
            if run_length > 0 {
                out.push((0b11 << 6) | ((run_length - 1) as u8));
                run_length = 0;
                continue;
            }

            let index_position = hash_pixel(&current_pixel);

            // Try encode QOI_OP_INDEX
            if hash_table[index_position] == current_pixel {
                out.push(index_position as u8);
                continue;
            }

            hash_table[index_position] = current_pixel;

            let dr = current_pixel[0].wrapping_sub(last_pixel[0]);
            let dg = current_pixel[1].wrapping_sub(last_pixel[1]);
            let db = current_pixel[2].wrapping_sub(last_pixel[2]);

            // Try encode QOI_OP_DIFF
            let dr_small = dr.wrapping_add(2);
            let dg_small = dg.wrapping_add(2);
            let db_small = db.wrapping_add(2);
            if dr_small <= 3 && dg_small <= 3 && db_small <= 3 {
                out.push((0b01 << 6) | (dr_small << 4) | (dg_small << 2) | (db_small << 0));
                last_pixel = current_pixel;
                continue;
            }

            // Try encode QOI_OP_LUMA
            let dg_big = dg.wrapping_add(32);
            let dr_dg_big = dr.wrapping_sub(dg_big).wrapping_add(8);
            let db_dg_big = db.wrapping_sub(dg_big).wrapping_add(8);
            if dg_big <= 63 && dr_dg_big <= 15 && db_dg_big <= 15 {
                out.push((0b10 << 6) | dg_big);
                out.push((dr_dg_big << 4) | db_dg_big);
                last_pixel = current_pixel;
                continue;
            }

            // Fallback encode QOI_OP_RGB
            // TODO: Check alpha hasn't changed. Else use QOI_OP_RGBA
            out.push(0b11111110);
            out.extend_from_slice(&current_pixel[0..3]);
            last_pixel = current_pixel;
        }

        if run_length > 0 {
            // Encode QOI_OP_RUN
            out.push((0b11 << 6) | ((run_length - 1) as u8));
            run_length = 0;
        }

        // End marker
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);

        Ok(())
    }
}
