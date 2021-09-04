#![feature(box_patterns, int_log)]

#[macro_use]
extern crate common;
extern crate crypto;
extern crate protobuf;
#[macro_use]
extern crate parsing;

pub mod buffer_queue;
pub mod deflate;
pub mod gzip;
pub mod huffman;
mod slice_reader;
pub mod snappy;
pub mod tar;
pub mod transform;
pub mod zip;
pub mod zlib;
