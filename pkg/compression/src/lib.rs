#![feature(box_patterns)]

extern crate protobuf;
#[macro_use] extern crate arrayref;
#[macro_use] extern crate parsing;

pub mod crc;
pub mod adler32;
pub mod huffman;
pub mod deflate;
pub mod gzip;
pub mod zlib;
pub mod snappy;