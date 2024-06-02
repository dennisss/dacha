/*

This is the major spec:
- https://www.itu.int/rec/T-REC-H.264-202108-I/en

H264 Byte streams
    https://learn.microsoft.com/en-us/windows/win32/directshow/h-264-video-types
    With start codes (Four CC 'H264')
    - List of NALUs
    - Each NALU prefixed by 0x000001 or 0x00000001

    Without start codes: Each NALU prefixed by a 1-4 byte length field (Four CC 'AVC1')

NAL (Network Abstraction Layer) Unit
- References:
    - https://yumichan.net/video-processing/video-compression/introduction-to-h264-nal-unit/


Decoding PPS/SPS
-

Storing H264 data in an MP4
-
- https://learn.microsoft.com/en-us/windows/win32/directshow/h-264-video-types
    - Witohut start codes

Debugging tools:
- https://mradionov.github.io/h264-bitstream-viewer/
- https://gpac.github.io/mp4box.js/test/filereader.html



Important MP4 atoms
- http://thompsonng.blogspot.com/2010/11/mp4-file-format-part-2.html
    - ISO IEC 14496-15
- https://b.goeswhere.com/W14837%20Carriage%20of%20AVC%20based%203D%20video%20excluding%20MVC%20final.pdf
- https://gist.github.com/yohhoy/2abc28b611797e7b407ae98faa7430e7
- 'avc1'
- 'avcC'
    - AVCDecoderConfigurationRecord

- In order to be seekable, we need 'stss'

Profile IDCs aredefined in https://en.wikipedia.org/wiki/Advanced_Video_Coding#Profiles


In RTP
- https://www.ietf.org/rfc/rfc3984
*/

mod proto {
    include!(concat!(env!("OUT_DIR"), "/src/h264.rs"));
}

pub use proto::*;

/// Iterates over an H264 bit/byte stream with start codes.
/// Currently assumes the complete stream is available in memory.
pub struct H264BitStreamIterator<'a> {
    remaining: &'a [u8],
    first: bool,
}

impl<'a> H264BitStreamIterator<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            remaining: data,
            first: true,
        }
    }

    pub fn peek<'b>(&'b mut self) -> Option<H264BitStreamIteratorPeek<'a, 'b>> {
        let mut remaining = self.remaining;
        let mut first = self.first;

        if remaining.is_empty() {
            return None;
        }

        let mut last_byte = 0xff;

        let mut i = 0;
        while i < remaining.len() {
            if i + 3 > remaining.len() {
                break;
            }

            if &remaining[i..(i + 3)] == &[0, 0, 1] {
                // Currently we will only support 4 byte start code sequences.
                assert_eq!(last_byte, 0);

                let data = &remaining[0..(i - 1)];

                // Skip start code
                remaining = &remaining[(i + 3)..];

                // We expect the start of the stream to have a start code.
                // So in this case, keep trying to find another start code.
                if first {
                    // Sometimes V4L2 cameras dumping H264 streams like to have a few unneeded
                    // bytes.
                    if !data.is_empty() {
                        eprintln!("Unused bytes before first NALU: {}", data.len());
                    }

                    first = false;
                    i = 0;
                    last_byte = 0xff;
                    continue;
                }

                return Some(H264BitStreamIteratorPeek {
                    iter: self,
                    data,
                    remaining,
                });
            }

            last_byte = remaining[i];
            i += 1;
        }

        assert!(!first);

        // Saw no start code, so return all the remaining data.
        Some(H264BitStreamIteratorPeek {
            iter: self,
            data: remaining,
            remaining: &[],
        })
    }

    pub fn next(&mut self) -> Option<&'a [u8]> {
        self.peek().map(|v| {
            let d = v.data();
            v.advance();
            d
        })
    }

    pub fn remaining(self) -> &'a [u8] {
        self.remaining
    }
}

pub struct H264BitStreamIteratorPeek<'a, 'b> {
    iter: &'b mut H264BitStreamIterator<'a>,
    data: &'a [u8],
    remaining: &'a [u8],
}

impl<'a, 'b> H264BitStreamIteratorPeek<'a, 'b> {
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn advance(mut self) {
        self.iter.remaining = self.remaining;
        self.iter.first = false;
    }
}
