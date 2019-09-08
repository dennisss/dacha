
extern crate byteorder;
extern crate compression;

use std::fs::File;
use std::io::{Read, Seek};
use common::errors::*;

use compression::crc::*;
use compression::bits::*;
use compression::huffman::*;
use compression::deflate::*;
use compression::gzip::*;
use compression::zlib::*;


// https://zlib.net/feldspar.html
// TODO: Blah b[D=5, L=18]!
// Should becode as 'Blah blah blah blah blah!'

// TODO: Implement zlib format https://www.ietf.org/rfc/rfc1950.txt

// TODO: Maintain a histogram of characters in the block to determine when to cut the block?

fn main() -> Result<()> {

	// let mut window = MatchingWindow::new();
	// let chars = b"Blah blah blah blah blah!";

	// let mut i = 0;
	// while i < chars.len() {
	// 	let mut n = 1;
	// 	if let Some(m) = window.find_match(&chars[i..]) {
	// 		println!("{:?}", m);
	// 		n = m.length;
	// 	} else {
	// 		println!("Literal: {}", chars[i] as char);
	// 	}

	// 	window.extend_from_slice(&chars[i..(i+n)]);
	// 	i += n;
	// }

	// assert_eq!(i, chars.len());

///
/*
	let header = Header {
		compression_method: CompressionMethod::Deflate,
		is_text: true,
		mtime: 10,
		extra_flags: 2, // < Max compression (slowest algorithm)
		os: GZIP_UNIX_OS,
		extra_field: None,
		filename: Some("lorem_ipsum.txt".into()),
		comment: None,
		header_validated: false
	};

	let mut infile = File::open("testdata/lorem_ipsum.txt")?;
	let mut indata = Vec::new();
	infile.read_to_end(&mut indata)?;

	
	let mut outfile = File::create("testdata/out/lorem_ipsum.txt.test.gz")?;
	write_gzip(header, &indata, &mut outfile)?;

	return Ok(());
*/
///

	let mut f = File::open("testdata/out/lorem_ipsum.txt.test.gz")?;


	let gz = read_gzip(&mut f)?;
	println!("{:?}", gz);

	// TODO: Assert that we now at the end of the file after reading.



	Ok(())
}