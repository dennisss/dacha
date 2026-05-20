use std::io::{Read, Write};

pub struct StuffedReader<'a, T: Read> {
    inner: &'a mut T,
}

impl<'a, T: Read> StuffedReader<'a, T> {
    pub fn new(inner: &'a mut T) -> Self {
        Self { inner }
    }
}

impl<'a, T: Read> Read for StuffedReader<'a, T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.len() != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Only reading one byte at a time is currently supported",
            ));
        }

        {
            let n = self.inner.read(buf)?;
            if n == 0 {
                return Ok(0);
            }
        }

        if buf[0] == 0xff {
            let mut temp = [0u8; 1];
            let n = self.inner.read(&mut temp)?;

            if n != 1 || temp[0] != 0x00 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Expected 0xFF to be stuffed by 0x00",
                ));
            }
        }

        Ok((1))
    }
}

pub struct StuffedWriter<'a> {
    inner: &'a mut Vec<u8>,
}

impl<'a> StuffedWriter<'a> {
    pub fn new(inner: &'a mut Vec<u8>) -> Self {
        Self { inner }
    }

    pub fn write(&mut self, buf: &[u8]) {
        for v in buf {
            self.inner.push(*v);

            if std::intrinsics::unlikely(*v == 0xff) {
                self.inner.push(0);
            }
        }
    }
}
