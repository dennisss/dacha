#![feature(box_patterns)]

extern crate protobuf;
extern crate crypto;
#[macro_use] extern crate arrayref;
#[macro_use] extern crate parsing;

pub mod huffman;
pub mod deflate;
pub mod gzip;
pub mod zlib;
pub mod snappy;