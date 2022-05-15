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

pub struct StuffedWriter<'a, T: Write> {
    inner: &'a mut T,
}

impl<'a, T: Write> StuffedWriter<'a, T> {
    pub fn new(inner: &'a mut T) -> Self {
        Self { inner }
    }
}

impl<'a, T: Write> Write for StuffedWriter<'a, T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for v in buf.iter().cloned() {
            self.inner.write(&[v])?;

            if v == 0xff {
                self.inner.write(&[0])?;
            }
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
