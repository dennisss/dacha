use common::errors::*;
use common::io::Readable;
use common::borrowed::Borrowed;
use parsing::ascii::AsciiString;
use parsing::iso::Latin1String;
use parsing::complete;

use crate::body::Body;
use crate::reader::*;
use crate::chunked_syntax::*;
use crate::header::Headers;

pub struct ChunkExtension {
    pub name: AsciiString,
    pub value: Option<Latin1String>,
}

pub struct ChunkHead {
    pub size: usize,
    pub extensions: Vec<ChunkExtension>,
}


/// Current state while reading a chunked body.
enum ChunkState {
    /// Reading the first line of the chunk containing the size.
    Start,
    
    /// Reading the data in the chunk.
    Data {
        remaining_len: usize
    },
    
    /// Done reading the data in the chunk and reading the empty line endings
    /// immediately after the data.
    End,
    
    /// Reading the final trailer of body until
    Trailer,
    
    /// The entire body has been read.
    Done {
        trailers: Option<Headers>
    },

    /// A previous attemtpt to read from the body caused a failure so we are in
    /// an undefined state and can't reliably continue reading from the body.
    Error,
}

enum CycleValue {
    StateChange,
    Read(usize),
}

pub struct IncomingChunkedBody {
    reader: Borrowed<PatternReader>,
    state: ChunkState,
}

impl IncomingChunkedBody {
    pub fn new(reader: Borrowed<PatternReader>) -> Self {
        IncomingChunkedBody {
            reader,
            state: ChunkState::Start,
        }
    }

    // TODO: Once an error occurs, then all sequential reads should also error out.
    async fn read_cycle(&mut self, buf: &mut [u8]) -> Result<CycleValue> {
        match self.state {
            ChunkState::Start => {
                let line = match self.reader.read_matching(LineMatcher::any()).await {
                    Ok(StreamReadUntil::Value(v)) => v,
                    Err(_) => {
                        return Err(err_msg("IO error while reading chunk start line"));
                    }
                    _ => {
                        return Err(err_msg("Expected chunk start line"));
                    }
                };

                let (head, _) = match complete(parse_chunk_start)(line) {
                    Ok(v) => v,
                    _ => {
                        return Err(err_msg("Invalid chunk start line"));
                    }
                };

                // TODO: Do something with the extensions

                if head.size == 0 {
                    self.state = ChunkState::Trailer;
                } else {
                    self.state = ChunkState::Data { remaining_len: head.size };
                }

                Ok(CycleValue::StateChange)
            }
            ChunkState::Data { remaining_len } => {
                // TODO: Also try reading = \r\n in the same call?
                let n = std::cmp::min(remaining_len, buf.len());
                let nread = self.reader.read(&mut buf[0..n]).await?;
                if nread == 0 && buf.len() > 0 {
                    return Err(err_msg("Reached end of stream before end of data"));
                }

                let new_len = remaining_len - nread;
                if new_len == 0 {
                    self.state = ChunkState::End;
                } else {
                    self.state = ChunkState::Data { remaining_len: new_len };
                }

                Ok(CycleValue::Read(nread))
            }
            ChunkState::End => {
                let mut buf = [0u8; 2];
                self.reader.read_exact(&mut buf).await?;
                if &buf != b"\r\n" {
                    return Err(err_msg("Expected CRLF after chunk data"));
                }

                self.state = ChunkState::Start;

                Ok(CycleValue::StateChange)
            }
            ChunkState::Trailer => {
                let data = match self.reader.read_matching(LineMatcher::empty()).await {
                    Ok(StreamReadUntil::Value(v)) => v,
                    Err(_) => {
                        return Err(err_msg("io error while reading trailer"));
                    }
                    _ => {
                        return Err(err_msg("Expected trailer empty line"));
                    }
                };

                let (headers, _) = match complete(parse_chunk_end)(data) {
                    Ok(v) => v,
                    _ => {
                        return Err(err_msg("Invalid chunk trailer"));
                    }
                };

                self.state = ChunkState::Done {
                    trailers: if headers.is_empty() { None } else { Some(Headers::from(headers)) }
                };

                Ok(CycleValue::StateChange)
            }
            ChunkState::Done { .. } => Ok(CycleValue::Read(0)),

            ChunkState::Error => {
                return Err(err_msg("Chunked body in an undefined state."));
            }
        }
    }
}

#[async_trait]
impl Body for IncomingChunkedBody {
    fn len(&self) -> Option<usize> {
        None
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        if let ChunkState::Done { trailers  } = &mut self.state {
            let trailers = trailers.take();
            self.state = ChunkState::Error;
            return Ok(trailers);
        }

        Err(err_msg("Trailers already read or body not fully read"))
    }
}

#[async_trait]
impl Readable for IncomingChunkedBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // TODO: Have a solution that doesn't require a loop here.
        loop {
            match self.read_cycle(buf).await {
                Ok(CycleValue::Read(n)) => {
                    return Ok(n);
                }
                Ok(CycleValue::StateChange) => {},
                Err(e) => {
                    self.state = ChunkState::Error;
                    return Err(e);
                }
            }
        }
    }
}

// TODO: Implement an OutgoingChunkedBody.
// - Will need to pick a chunk size and be able to flush from upstream.


#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use super::*;

    const TEST_BODY: &'static [u8] = b"7\r\nMozilla\r\n9\r\nDeveloper\r\n7\r\nNetwork\r\n0\r\n\r\n";

    #[async_std::test]
    async fn chunked_body_test() -> Result<()> {
        let data = Cursor::new(TEST_BODY);
        let stream = PatternReader::new(Box::new(data), StreamBufferOptions::default());
        let (stream, returner) = common::borrowed::Borrowed::wrap(stream);
        
        let mut body = IncomingChunkedBody::new(stream);

        let mut outbuf = vec![];
        body.read_to_end(&mut outbuf).await?;

        assert_eq!(&outbuf, b"MozillaDeveloperNetwork");

        Ok(())
    }
}
