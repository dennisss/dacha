
#![feature(box_patterns)]

extern crate byteorder;

use std::fs::File;
use std::io::{Read, Seek};
use common::errors::*;

mod crc;
mod bits;
mod huffman;
mod deflate;
mod gzip;

use crc::*;
use bits::*;
use huffman::*;
use deflate::*;
use gzip::*;


// https://zlib.net/feldspar.html
// TODO: Blah b[D=5, L=18]!
// Should becode as 'Blah blah blah blah blah!'

// TODO: Implement zlib format https://www.ietf.org/rfc/rfc1950.txt

// TODO: Maintain a histogram of characters in the block to determine when to cut the block?

fn main() -> Result<()> {

	let mut window = MatchingWindow::new();
	let chars = b"Blah blah blah blah blah!";

	let mut i = 0;
	while i < chars.len() {
		let mut n = 1;
		if let Some(m) = window.find_match(&chars[i..]) {
			println!("{:?}", m);
			n = m.length;
		} else {
			println!("Literal: {}", chars[i] as char);
		}

		window.extend_from_slice(&chars[i..(i+n)]);
		i += n;
	}

	assert_eq!(i, chars.len());


	let data = vec![1, 2,2, 3,3,3, 4,4,4,4, 5,5,5,5, 6,6,6,6,6,6 ];
	println!("{:?}", HuffmanTree::build_length_limited_tree(&data, 3)?);

	return Ok(());


	let mut f = File::open("testdata/lorem_ipsum.txt.gz")?;

	let gz = read_gzip(&mut f)?;
	println!("{:?}", gz);

	// TODO: Don't allow reading beyond end of range
	f.seek(std::io::SeekFrom::Start(gz.compressed_range.0))?;
	
	// Next step is to validate the CRC and decompressed size?
	// Also must implement as an incremental state machine using async/awaits!

	read_inflate(&mut f)?;




	Ok(())
}