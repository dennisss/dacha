use crate::{Colorspace, Image};
use byteorder::{LittleEndian, ReadBytesExt};
use common::errors::*;
use math::array::Array;
use parsing::cstruct::*;
use parsing::ParseResult;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub struct Bitmap {
    pub image: Image<u8>,
}

const MAGIC: &[&'static str] = &["BM", "BA", "CI", "CP", "IC", "PT"];

const BITMAPCOREHEADER_SIZE: u32 = 12;
const BITMAPV5HEADER_SIZE: u32 = 124;

// BITMAPCOREHEADER
// OS21XBITMAPHEADER
#[derive(Reflect, Debug, Clone, Default)]
struct BitmapCoreHeader {
    width_px: u16,
    height_px: u16,
    num_color_planes: u16,
    num_bit_per_pixel: u16,
}

impl BitmapCoreHeader {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut value = Self::default();
        let rest = parse_cstruct_le(input, &mut value)?;
        Ok((value, rest))
    }
}

// BITMAPINFOHEADER
#[derive(Reflect, Debug, Clone, Default)]
struct BitmapInfoHeader {
    width_px: i32,
    height_px: i32,
    num_color_planes: u16,
    num_bit_per_pixel: u16,
    compression_method: u32,
    image_size: u32,
    horiz_res: i32,
    vert_res: i32,
    num_colors: u32,
    num_important_colors: u32,
}

impl BitmapInfoHeader {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut value = Self::default();
        let rest = parse_cstruct_le(input, &mut value)?;
        Ok((value, rest))
    }
}

impl Bitmap {
    pub fn open(path: &str) -> Result<Bitmap> {
        let mut file = File::open(path)?;

        let mut header = [0u8; 14];
        file.read_exact(&mut header)?;

        let mut found_magic = false;
        for m in MAGIC {
            if m.as_bytes() == &header[0..2] {
                found_magic = true;
                break;
            }
        }

        assert!(found_magic);

        let file_size = (&header[2..]).read_u32::<LittleEndian>()?;
        let start_off = (&header[10..]).read_u32::<LittleEndian>()?;

        println!("File size: {}\tStart offset: {}", file_size, start_off);

        let dib_header_size = file.read_u32::<LittleEndian>()?;
        println!("DIB header size: {}", dib_header_size);

        let mut dib_header = Vec::new();
        dib_header.resize((dib_header_size - 4) as usize, 0); // Header size - size of int read above
        file.read_exact(&mut dib_header)?;

        if dib_header_size == BITMAPCOREHEADER_SIZE {
            let (header, rest) = match BitmapCoreHeader::parse(&dib_header) {
                Ok(v) => v,
                Err(_) => return Err(err_msg("Failed to parse BMP")),
            };

            println!("{:?}", header);
        }
        // BITMAPINFOHEADER
        else if dib_header_size == 40 {
        }
        // BITMAPV4HEADER
        else if dib_header_size == 108 {
            let (header, rest) = match BitmapInfoHeader::parse(&dib_header) {
                Ok(v) => v,
                Err(_) => return Err(err_msg("Failed to parse BMP")),
            };

            println!("{:?}", header);
            assert_eq!(header.num_color_planes, 1); // Should always be this way
            assert_eq!(header.compression_method, 0); // No compression
            assert_eq!(header.num_bit_per_pixel, 24); // Standard RGB
                                                      // TODO: Also check that the color table is empty

            let mut data = Vec::new();
            data.resize((header.width_px * header.height_px * 3) as usize, 0);
            file.seek(SeekFrom::Start(start_off as u64))?;
            file.read_exact(&mut data)?;

            let mut arr = Array::<u8> {
                shape: vec![header.height_px as usize, header.width_px as usize, 3],
                data,
            };

            return Ok(Bitmap {
                image: Image {
                    array: arr.flip(0),
                    colorspace: Colorspace::RGB,
                },
            });
        } else {
            panic!("Unsupported header of size: {}", dib_header_size);
        }

        Err(err_msg("Failed to load image"))
        // Ok(Bitmap {})
    }
}
