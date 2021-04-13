

DEFLATE:
- https://tools.ietf.org/html/rfc1951
- Little endian numbers

Zlib's algorithm specifics:
https://github.com/madler/zlib/blob/master/doc/algorithm.txt


Compression APIs in rust:
- `deflate` crate:
	- Constructor takes a writer as input.
	- Incrementally write data into the struct and it forwards it into the 

Traditional C APIs
- Input buffer and output buffer given



Node.js API
- Write chunk into a stream
- Occasionally buffers are emmited


Buffers:
- Ring buffer of uncompressed data
- For deflate
	- Buffer of uncompressed data waiting to be compressed
- For inflate
	- Buffer of compressed data that is too small to be decompressed


in zlib memLevel mainly effects the size of the lit buffer (aka how many bytes are accumulated before compression actually starts)

- Ideally we would directly run-length encode, but that would be annoying


Summary of compression modes:

- Small in-memory compression/decompression
	- Keep providing input data to the codec
	- Don't take any intermediate output data (but limit the size of the codec buffer)
	- After finished providing all input, take ownership of entire output buffer with zero additional copies
- Traditional streaming workload
	- Provide input to the codec
	- Take all output that the codec will voluntarily provide (in a while loop)
	- In this mode, the output is copied incrementally from the codec's internal output buffer to the user provided output buffer
- Large chunks no copies
	- Provide input to the codec
	- Take internal reference to the 