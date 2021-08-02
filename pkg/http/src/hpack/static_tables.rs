// Contains static data used by the HPACK algorithm.

use common::{bits::BitVector, errors::*};
use compression::huffman::HuffmanTree;
use parsing::binary::be_u8;
use parsing::parse_next;

use crate::hpack::header_field::HeaderFieldRef;

macro_rules! static_table {
    ($($i:expr, $name:expr, $value: expr),+) => {
        &[
            $(HeaderFieldRef { /* index: $i, */ name: $name.as_bytes(), value: $value.as_bytes() }),+
        ]
    };
}

#[rustfmt::skip]
pub const STATIC_TABLE: &'static [HeaderFieldRef<'static>] = static_table!(
    1, ":authority", "",
    2, ":method", "GET",
    3, ":method", "POST",
    4, ":path", "/",
    5, ":path", "/index.html",
    6, ":scheme", "http",
    7, ":scheme", "https",
    8, ":status", "200",
    9, ":status", "204",
    10, ":status", "206",
    11, ":status", "304",
    12, ":status", "400",
    13, ":status", "404",
    14, ":status", "500",
    15, "accept-charset", "",
    16, "accept-encoding", "gzip, deflate",
    17, "accept-language", "",
    18, "accept-ranges", "",
    19, "accept", "",
    20, "access-control-allow-origin", "",
    21, "age", "",
    22, "allow", "",
    23, "authorization", "",
    24, "cache-control", "",
    25, "content-disposition", "",
    26, "content-encoding", "",
    27, "content-language", "",
    28, "content-length", "",
    29, "content-location", "",
    30, "content-range", "",
    31, "content-type", "",
    32, "cookie", "",
    33, "date", "",
    34, "etag", "",
    35, "expect", "",
    36, "expires", "",
    37, "from", "",
    38, "host", "",
    39, "if-match", "",
    40, "if-modified-since", "",
    41, "if-none-match", "",
    42, "if-range", "",
    43, "if-unmodified-since", "",
    44, "last-modified", "",
    45, "link", "",
    46, "location", "",
    47, "max-forwards", "",
    48, "proxy-authenticate", "",
    49, "proxy-authorization", "",
    50, "range", "",
    51, "referer", "",
    52, "refresh", "",
    53, "retry-after", "",
    54, "server", "",
    55, "set-cookie", "",
    56, "strict-transport-security", "",
    57, "transfer-encoding", "",
    58, "user-agent", "",
    59, "vary", "",
    60, "via", "",
    61, "www-authenticate", ""
);

/// Compact representation of a huffman code (aka just a bit vector) as a
/// integer 'code' whose lower 'length' bits store the code.
#[repr(packed)]
struct HuffmanCodeEntry {
    code: u32,
    length: u8, // TODO: I could encode this in the u32 as the last 1 bit.
}

macro_rules! huffman_table {
    ($($code:tt, $length:expr),+) => {
        &[
            $(HuffmanCodeEntry { code: $code, length: $length }),+
        ]
    };
}

/// The (code, length) to encode a byte value 'i' is located at index 'i'.
/// A value of 256 is the End of String symbol.
/// RFC 7541: Appendix B
#[rustfmt::skip]
const HUFFMAN_CODES: &[HuffmanCodeEntry; 256] = huffman_table!(
    0b1111111111000, 13,
    0b11111111111111111011000, 23,
    0b1111111111111111111111100010, 28,
    0b1111111111111111111111100011, 28,
    0b1111111111111111111111100100, 28,
    0b1111111111111111111111100101, 28,
    0b1111111111111111111111100110, 28,
    0b1111111111111111111111100111, 28,
    0b1111111111111111111111101000, 28,
    0b111111111111111111101010, 24,
    0b111111111111111111111111111100, 30,
    0b1111111111111111111111101001, 28,
    0b1111111111111111111111101010, 28,
    0b111111111111111111111111111101, 30,
    0b1111111111111111111111101011, 28,
    0b1111111111111111111111101100, 28,
    0b1111111111111111111111101101, 28,
    0b1111111111111111111111101110, 28,
    0b1111111111111111111111101111, 28,
    0b1111111111111111111111110000, 28,
    0b1111111111111111111111110001, 28,
    0b1111111111111111111111110010, 28,
    0b111111111111111111111111111110, 30,
    0b1111111111111111111111110011, 28,
    0b1111111111111111111111110100, 28,
    0b1111111111111111111111110101, 28,
    0b1111111111111111111111110110, 28,
    0b1111111111111111111111110111, 28,
    0b1111111111111111111111111000, 28,
    0b1111111111111111111111111001, 28,
    0b1111111111111111111111111010, 28,
    0b1111111111111111111111111011, 28,
    0b010100, 6,
    0b1111111000, 10,
    0b1111111001, 10,
    0b111111111010, 12,
    0b1111111111001, 13,
    0b010101, 6,
    0b11111000, 8,
    0b11111111010, 11,
    0b1111111010, 10,
    0b1111111011, 10,
    0b11111001, 8,
    0b11111111011, 11,
    0b11111010, 8,
    0b010110, 6,
    0b010111, 6,
    0b011000, 6,
    0b00000, 5,
    0b00001, 5,
    0b00010, 5,
    0b011001, 6,
    0b011010, 6,
    0b011011, 6,
    0b011100, 6,
    0b011101, 6,
    0b011110, 6,
    0b011111, 6,
    0b1011100, 7,
    0b11111011, 8,
    0b111111111111100, 15,
    0b100000, 6,
    0b111111111011, 12,
    0b1111111100, 10,
    0b1111111111010, 13,
    0b100001, 6,
    0b1011101, 7,
    0b1011110, 7,
    0b1011111, 7,
    0b1100000, 7,
    0b1100001, 7,
    0b1100010, 7,
    0b1100011, 7,
    0b1100100, 7,
    0b1100101, 7,
    0b1100110, 7,
    0b1100111, 7,
    0b1101000, 7,
    0b1101001, 7,
    0b1101010, 7,
    0b1101011, 7,
    0b1101100, 7,
    0b1101101, 7,
    0b1101110, 7,
    0b1101111, 7,
    0b1110000, 7,
    0b1110001, 7,
    0b1110010, 7,
    0b11111100, 8,
    0b1110011, 7,
    0b11111101, 8,
    0b1111111111011, 13,
    0b1111111111111110000, 19,
    0b1111111111100, 13,
    0b11111111111100, 14,
    0b100010, 6,
    0b111111111111101, 15,
    0b00011, 5,
    0b100011, 6,
    0b00100, 5,
    0b100100, 6,
    0b00101, 5,
    0b100101, 6,
    0b100110, 6,
    0b100111, 6,
    0b00110, 5,
    0b1110100, 7,
    0b1110101, 7,
    0b101000, 6,
    0b101001, 6,
    0b101010, 6,
    0b00111, 5,
    0b101011, 6,
    0b1110110, 7,
    0b101100, 6,
    0b01000, 5,
    0b01001, 5,
    0b101101, 6,
    0b1110111, 7,
    0b1111000, 7,
    0b1111001, 7,
    0b1111010, 7,
    0b1111011, 7,
    0b111111111111110, 15,
    0b11111111100, 11,
    0b11111111111101, 14,
    0b1111111111101, 13,
    0b1111111111111111111111111100, 28,
    0b11111111111111100110, 20,
    0b1111111111111111010010, 22,
    0b11111111111111100111, 20,
    0b11111111111111101000, 20,
    0b1111111111111111010011, 22,
    0b1111111111111111010100, 22,
    0b1111111111111111010101, 22,
    0b11111111111111111011001, 23,
    0b1111111111111111010110, 22,
    0b11111111111111111011010, 23,
    0b11111111111111111011011, 23,
    0b11111111111111111011100, 23,
    0b11111111111111111011101, 23,
    0b11111111111111111011110, 23,
    0b111111111111111111101011, 24,
    0b11111111111111111011111, 23,
    0b111111111111111111101100, 24,
    0b111111111111111111101101, 24,
    0b1111111111111111010111, 22,
    0b11111111111111111100000, 23,
    0b111111111111111111101110, 24,
    0b11111111111111111100001, 23,
    0b11111111111111111100010, 23,
    0b11111111111111111100011, 23,
    0b11111111111111111100100, 23,
    0b111111111111111011100, 21,
    0b1111111111111111011000, 22,
    0b11111111111111111100101, 23,
    0b1111111111111111011001, 22,
    0b11111111111111111100110, 23,
    0b11111111111111111100111, 23,
    0b111111111111111111101111, 24,
    0b1111111111111111011010, 22,
    0b111111111111111011101, 21,
    0b11111111111111101001, 20,
    0b1111111111111111011011, 22,
    0b1111111111111111011100, 22,
    0b11111111111111111101000, 23,
    0b11111111111111111101001, 23,
    0b111111111111111011110, 21,
    0b11111111111111111101010, 23,
    0b1111111111111111011101, 22,
    0b1111111111111111011110, 22,
    0b111111111111111111110000, 24,
    0b111111111111111011111, 21,
    0b1111111111111111011111, 22,
    0b11111111111111111101011, 23,
    0b11111111111111111101100, 23,
    0b111111111111111100000, 21,
    0b111111111111111100001, 21,
    0b1111111111111111100000, 22,
    0b111111111111111100010, 21,
    0b11111111111111111101101, 23,
    0b1111111111111111100001, 22,
    0b11111111111111111101110, 23,
    0b11111111111111111101111, 23,
    0b11111111111111101010, 20,
    0b1111111111111111100010, 22,
    0b1111111111111111100011, 22,
    0b1111111111111111100100, 22,
    0b11111111111111111110000, 23,
    0b1111111111111111100101, 22,
    0b1111111111111111100110, 22,
    0b11111111111111111110001, 23,
    0b11111111111111111111100000, 26,
    0b11111111111111111111100001, 26,
    0b11111111111111101011, 20,
    0b1111111111111110001, 19,
    0b1111111111111111100111, 22,
    0b11111111111111111110010, 23,
    0b1111111111111111101000, 22,
    0b1111111111111111111101100, 25,
    0b11111111111111111111100010, 26,
    0b11111111111111111111100011, 26,
    0b11111111111111111111100100, 26,
    0b111111111111111111111011110, 27,
    0b111111111111111111111011111, 27,
    0b11111111111111111111100101, 26,
    0b111111111111111111110001, 24,
    0b1111111111111111111101101, 25,
    0b1111111111111110010, 19,
    0b111111111111111100011, 21,
    0b11111111111111111111100110, 26,
    0b111111111111111111111100000, 27,
    0b111111111111111111111100001, 27,
    0b11111111111111111111100111, 26,
    0b111111111111111111111100010, 27,
    0b111111111111111111110010, 24,
    0b111111111111111100100, 21,
    0b111111111111111100101, 21,
    0b11111111111111111111101000, 26,
    0b11111111111111111111101001, 26,
    0b1111111111111111111111111101, 28,
    0b111111111111111111111100011, 27,
    0b111111111111111111111100100, 27,
    0b111111111111111111111100101, 27,
    0b11111111111111101100, 20,
    0b111111111111111111110011, 24,
    0b11111111111111101101, 20,
    0b111111111111111100110, 21,
    0b1111111111111111101001, 22,
    0b111111111111111100111, 21,
    0b111111111111111101000, 21,
    0b11111111111111111110011, 23,
    0b1111111111111111101010, 22,
    0b1111111111111111101011, 22,
    0b1111111111111111111101110, 25,
    0b1111111111111111111101111, 25,
    0b111111111111111111110100, 24,
    0b111111111111111111110101, 24,
    0b11111111111111111111101010, 26,
    0b11111111111111111110100, 23,
    0b11111111111111111111101011, 26,
    0b111111111111111111111100110, 27,
    0b11111111111111111111101100, 26,
    0b11111111111111111111101101, 26,
    0b111111111111111111111100111, 27,
    0b111111111111111111111101000, 27,
    0b111111111111111111111101001, 27,
    0b111111111111111111111101010, 27,
    0b111111111111111111111101011, 27,
    0b1111111111111111111111111110, 28,
    0b111111111111111111111101100, 27,
    0b111111111111111111111101101, 27,
    0b111111111111111111111101110, 27,
    0b111111111111111111111101111, 27,
    0b111111111111111111111110000, 27,
    0b11111111111111111111101110, 26

    // NOTE: This is the End Of String symbol, but we never actually need to encode it.
    // 0b111111111111111111111111111111, 30
);

lazy_static! {
    pub static ref HUFFMAN_TREE: HuffmanTree = {
        let mut tree = HuffmanTree::new();
        for (i, entry) in HUFFMAN_CODES.iter().enumerate() {
            // TODO: from_lower_msb may lose precision is usize is < 30 bits.
            tree.insert(i, BitVector::from_lower_msb(entry.code as usize, entry.length)).unwrap();
        }

        tree
    };
}

/// Optimized method fo huffman encode some input data using the static HPACK
/// code table. Because all codes are 30bits or less, we can fully operate on
/// them using 64-bit slices at a time.
///
/// TODO: Consolidate this with the generic huffman libraries.
pub fn huffman_encode(input: &[u8]) -> Vec<u8> {
    const U64_BYTES: usize = std::mem::size_of::<u64>();
    const U64_BITS: usize = U64_BYTES * 8;

    let mut out = vec![];
    out.resize(U64_BYTES, 0);

    let mut bit_offset = 0;

    for byte in input.iter().cloned() {
        let byte_offset = bit_offset / 8;
        let bit_rel_offset = bit_offset % 8;

        // Exponentially expand the size of the output buffer to ensure that at least a
        // single u64 can fit without truncation at the current position.
        if out.len() - byte_offset < U64_BYTES {
            out.resize(std::cmp::max(2 * out.len(), out.len() + U64_BYTES), 0);
        }

        // Read a u64 view from the current position.
        let out_slice = array_mut_ref![out, byte_offset, U64_BYTES];
        let mut out_int = u64::from_be_bytes(*out_slice);

        // Insert the code
        let code_entry = &HUFFMAN_CODES[byte as usize];
        out_int |=
            (code_entry.code as u64) << (U64_BITS - bit_rel_offset - (code_entry.length as usize));

        // Store the u64 view back into the output buffer.
        *out_slice = out_int.to_be_bytes();

        bit_offset += code_entry.length as usize;
    }

    // Finalize the output buffer by truncating/filling it to the final size.
    {
        let byte_offset = bit_offset / 8;
        let bit_rel_offset = bit_offset % 8;

        if bit_rel_offset != 0 {
            out.truncate(byte_offset + 1);
            // Set all unused bits in the final byte to 1's.
            out[byte_offset] |= (1 << (8 - bit_rel_offset)) - 1;
        } else {
            out.truncate(byte_offset);
            // No need for masking as the final byte it full.
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hpack_static_find_perfect_hash() {
        let mut seen_hashes = std::collections::HashMap::new();

        for entry in STATIC_TABLE {
            // TODO: We an also fast threshold on the length of the name.
            let mut hash = entry.name.len();
            for (i, b) in entry.name.iter().enumerate() {
                hash += (*b as usize) << i;
            }

            if let Some(old_name) = seen_hashes.insert(hash, entry.name) {
                if old_name != entry.name {
                    panic!(
                        "Duplicate entries with same hash: {:?} {:?}",
                        old_name, entry.name
                    );
                }
            }
        }

        // Now we need to brute force some mapping from the hash to the right starting
        // index.

        println!("{:?}", seen_hashes);

        println!("{}", seen_hashes.len());
    }
}
