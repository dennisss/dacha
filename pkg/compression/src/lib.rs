#![feature(box_patterns)]

#[macro_use] extern crate common;
extern crate crypto;
extern crate protobuf;
#[macro_use] extern crate parsing;

pub mod transform;
pub mod deflate;
pub mod gzip;
pub mod huffman;
pub mod snappy;
pub mod zlib;
pub mod zip;
pub mod buffer_queue;
mod slice_reader;
pub mod tar;