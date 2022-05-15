pub const START_OF_IMAGE: &[u8] = &[0xff, 0xd8]; // SOI
pub const END_OF_IMAGE: u8 = 0xd9; // EOI

pub const APP0: u8 = 0xe0;

// Start Of Frame markers, non-differential, Huffman coding
pub const SOF0: u8 = 0xC0; // Baseline DCT
pub const SOF1: u8 = 0xC1; // Extended sequential DCT
pub const SOF2: u8 = 0xC2; // Progressive DCT
pub const SOF3: u8 = 0xC3; // Lossless (sequential)

/// Define Arithmetic Coding Conditioning Table(s)
pub const DAC: u8 = 0xCC;

/// Define Huffman Table
pub const DHT: u8 = 0xC4;

/// Define Quantization Table
pub const DQT: u8 = 0xDB;

/// Define Restart Interval
pub const DRI: u8 = 0xDD;

pub const START_OF_SCAN: u8 = 0xda; // SOS
