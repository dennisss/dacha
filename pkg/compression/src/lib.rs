#![feature(box_patterns, int_log, is_symlink)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate crypto;
extern crate protobuf;
#[macro_use]
extern crate parsing;
#[macro_use]
extern crate file;
#[macro_use]
extern crate macros;

pub mod buffer_queue;
pub mod deflate;
pub mod gzip;
pub mod huffman;
pub mod readable;
mod slice_reader;
pub mod snappy;
pub mod tar;
pub mod transform;
pub mod zip;
pub mod zlib;
