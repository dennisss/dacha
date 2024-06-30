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
}

impl<'a> H264BitStreamIterator<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { remaining: data }
    }

    pub fn peek<'b>(&'b mut self) -> Option<H264BitStreamIteratorPeek<'a, 'b>> {
        let mut remaining = self.remaining;

        if remaining.is_empty() {
            return None;
        }

        let (start_index, start_len) = match Self::find_start_code(remaining) {
            Some(v) => v,
            // TODO: Log how many skipped bytes there are.
            None => return None,
        };

        let data_start_index = start_index + start_len;

        // TODO: Avoid re-calculating this each time.
        let end_index = match Self::find_start_code(&remaining[data_start_index..]) {
            Some((v, _)) => data_start_index + v,
            None => remaining.len(),
        };

        // Sometimes V4L2 cameras dumping H264 streams like to have a few unneeded
        // bytes before the first start code.
        if start_index != 0 {
            eprintln!("Unused bytes before first NALU: {}", start_index);
        }

        let raw = &remaining[start_index..end_index];
        let data = &remaining[data_start_index..end_index];
        let remaining = &remaining[end_index..];

        Some(H264BitStreamIteratorPeek {
            iter: self,
            raw,
            data,
            remaining,
        })
    }

    /// Returns the start index and code length
    fn find_start_code(data: &[u8]) -> Option<(usize, usize)> {
        for i in 0..data.len() {
            let mut code_length = 0;
            if data[i..].starts_with(&[0, 0, 1]) {
                return Some((i, 3));
            } else if data[i..].starts_with(&[0, 0, 0, 1]) {
                return Some((i, 4));
            }
        }

        None
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
    raw: &'a [u8],
    data: &'a [u8],
    remaining: &'a [u8],
}

impl<'a, 'b> H264BitStreamIteratorPeek<'a, 'b> {
    /// Gets all the data in the current packet (start code + NALU data)
    pub fn raw(&self) -> &'a [u8] {
        self.raw
    }

    /// Gets the NALU data in the current packet (start code stripped).
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn advance(mut self) {
        self.iter.remaining = self.remaining;
    }
}
