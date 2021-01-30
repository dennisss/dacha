#[macro_use]
extern crate automata;
#[macro_use]
extern crate common;
#[macro_use]
extern crate parsing;

pub mod ben;

struct Metainfo {
    announce: String,
    info: MetainfoInfo,
}

struct MetainfoInfo {
    name: String,
    piece_length: u64,
    pieces: Vec<u8>,
    length: Option<u64>,

    // NOTE: THis should be empty when length is specified (and vice versa)
    files: Vec<MetainfoFile>,
}

struct MetainfoFile {
    length: u64,
    path: Vec<String>,
}

struct TrackerRequest {
    info_hash: [u8; 20],
    peer_id: [u8; 20],
    ip: String,
    port: u16,
    uploaded: u64,
    downloaded: u64,
    left: u64,
    event: String, // "empty"
}

struct TrackerResponse {
    failure_reason: Option<String>,
}
