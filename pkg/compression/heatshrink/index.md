# Heatshrink Compression Algorithm

This is a Rust implementation that is compatible with the implementation [here](https://github.com/atomicobject/heatshrink/tree/master).

## Bitstream Specification

Encoder/decoder parameters:

- `window_bits`: Number of bits used to store an index in the sliding window.
- `lookahead_bits`: Number of bits used to store a backreference length.

The output bit stream is formed by appending sequences of bits where bytes are appended with the following ordering rules:

- When a new byte is created on the output stream, bits are appended first to the highest bit (0x80 position) and then are appended all the way  down to the lowest bit (0x01 position).
- When a sequence of >1 bits is being added, those bits are also read from highest to lowest bit order.
- Multi-byte integers are similarly appended in big-endian ordering (starting with the most significant bits and going down).

Bit sequences are composed as follows:

- Tag bit (first bit):
    - `1`: Indicates a literal byte. The next 8 bits in the stream are an uncompressed literal.
    - `0`: Backreference
        - Next `window_bits` bits are the 'backreference distance - 1'.
        - Next `lookahead_bits` bits are the 'backreference length - 1'.

For `bgcode`, supported parameters are:

- window size = 11, lookahead size = 4
- window size = 12, lookahead size = 4

