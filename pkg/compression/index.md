# Compression

Library for performing compression/decompression of data either as:

1. Data streams using raw codecs like Deflate/Snappy/GZip/Zstd.
2. Archives storing multiple files (e.g. Zip/Tar)

## Stream Codecs

Encoders/decoders of byte streams all implement the `crate::Transform` trait which supports incremental processing of (un-)compressed inputs chunk by chunk. This interface is designed with the use-case data not being able to fit completely in memory all at once. For cases where compression/decompression of small buffers, use the `transform_to_vec()` helper which aims to properly hint to the codec that all input/output buffers are available right away.

## References

DEFLATE:
- https://tools.ietf.org/html/rfc1951
- Little endian numbers

Zlib's algorithm specifics:
- https://github.com/madler/zlib/blob/master/doc/algorithm.txt
