use super::super::errors::*;
use super::{Image, Array};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use byteorder::{LittleEndian, ReadBytesExt};
use nom::{le_u16, le_i16, le_i32, le_u32};


pub struct Bitmap {

	pub image: Image<u8>
}

// 

const MAGIC: &[&'static str] = &["BM", "BA", "CI", "CP", "IC", "PT"];

const BITMAPCOREHEADER_SIZE: u32 = 12;
const BITMAPV5HEADER_SIZE: u32 = 124;

// BITMAPCOREHEADER
// OS21XBITMAPHEADER
#[derive(Debug, Clone)]
struct BitmapCoreHeader {
	width_px: u16,
	height_px: u16,
	num_color_planes: u16,
	num_bit_per_pixel: u16
}

named!(bitmap_core_header<&[u8], BitmapCoreHeader>, do_parse!(
	width_px: le_u16 >>
	height_px: le_u16 >>
	num_color_planes: le_u16 >>
	num_bit_per_pixel: le_u16 >>
	(BitmapCoreHeader { width_px, height_px, num_color_planes, num_bit_per_pixel })
));


// BITMAPINFOHEADER
#[derive(Debug, Clone)]
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
	num_important_colors: u32
}

named!(bitmap_info_header<&[u8], BitmapInfoHeader>, do_parse!(
	width_px: le_i32 >>
	height_px: le_i32 >>
	num_color_planes: le_u16 >>
	num_bit_per_pixel: le_u16 >>
	compression_method: le_u32 >>
	image_size: le_u32 >>
	horiz_res: le_i32 >>
	vert_res: le_i32 >>
	num_colors: le_u32 >>
	num_important_colors: le_u32 >>
	(BitmapInfoHeader { width_px, height_px, num_color_planes, num_bit_per_pixel, compression_method, image_size, horiz_res, vert_res, num_colors, num_important_colors })
));


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
			let (rest, header) = match bitmap_core_header(&dib_header) {
				Ok(v) => v,
				Err(_) => return Err("Failed to parse BMP".into())
			};

			println!("{:?}", header);
		}
		// BITMAPINFOHEADER
		else if dib_header_size == 40 {
			
		}
		// BITMAPV4HEADER
		else if dib_header_size == 108 {
			let (rest, header) = match bitmap_info_header(&dib_header) {
				Ok(v) => v,
				Err(_) => return Err("Failed to parse BMP".into())
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
				data
			};

			return Ok(Bitmap {
				image: Image {
					array: arr.flip(0)
				}
			});

		}
		else {
			panic!("Unsupported header of size: {}", dib_header_size);
		}

		Err("Failed to load image".into())
		// Ok(Bitmap {})
	}
}