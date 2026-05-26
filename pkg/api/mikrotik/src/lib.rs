#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::{convert::TryFrom, str::FromStr};

use common::errors::*;
use net::tcp::TcpStream;
use common::io::*;


pub struct Client {
    stream: TcpStream,
}

impl Client {
    pub async fn connect_insecure(addr: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr.parse()?).await?;
        Ok(Self { stream })
    }

    pub async fn request<T: AsRef<[u8]>>(&mut self, req: &[T]) -> Result<Vec<String>> {
        let mut sentence = vec![];
        for v in req {
            let v = v.as_ref();
            if v.len() == 0 {
                return Err(err_msg("Request words should be non-empty"));
            }            

            serialize_word(v, &mut sentence);
        }

        serialize_word(&[], &mut sentence);
        self.stream.write_all(&sentence).await?;


        let mut buf = vec![0u8; 4096];
        let mut buf_size = 0;

        loop {
            if buf_size == buf.len() {
                return Err(err_msg("Reply is too long"));
            }

            let n = self.stream.read(&mut buf[buf_size..]).await?;
            if n == 0 {
                return Err(err_msg("Hit end of reply stream"));
            }

            buf_size += n;

            // Attempt to parse it.
            {
                let mut words = vec![];
                let mut reply_complete = false;

                let mut rest = &buf[0..buf_size];
                while !rest.is_empty() {
                    let (len, r) = parse_length(rest)?;
                    rest = r;

                    if rest.len() < len {
                        break;
                    }

                    if len == 0 {
                        reply_complete = true;
                        break;
                    }

                    let w = &rest[0..len];
                    words.push(std::str::from_utf8(w)?.to_string());
                    rest = &rest[len..];
                }

                if reply_complete {
                    if rest.len() != 0 {
                        return Err(err_msg("Extra bytes after end of reply sentence"));
                    }

                    return Ok(words);
                }
            }
        }
    }

    pub async fn login(&mut self, user: &str, pass: &str) -> Result<()> {
        let reply = self.request(&[
            "/login".to_string(),
            format!("=name={}", user),
            format!("=password={}", pass),
        ]).await?;
        
        if reply != &["!done"] {
            return Err(format_err!("Login failed: {:?}", reply));
        }

        Ok(())
    }

}

fn serialize_length(v: usize, out: &mut Vec<u8>) {
    assert!(v <= 0x7FFFFFFFFF);
    let mut bytes = (v as u32).to_be_bytes();

    // 1 byte long
    if v <= 0x7F {
        out.push(bytes[3]);
        return;
    }

    // 2 bytes long
    if v <= 0x3FFF {
        bytes[2] |= 0x80;
        out.extend_from_slice(&bytes[2..]);
        return;
    }

    // 3 bytes
    if v <= 0x1FFFFF {
        bytes[1] |= 0xC0;
        out.extend_from_slice(&bytes[1..]);
        return;
    }
    
    if v <= 0xFFFFFFF {
        bytes[0] |= 0xE0;
        out.extend_from_slice(&bytes[0..]);
        return;
    }

    out.push(0xF0);
    out.extend_from_slice(&bytes[..]);
}

fn parse_length<'a>(data: &'a [u8]) -> Result<(usize, &'a [u8])> {
    if data.len() < 1 {
        return Err(err_msg("Too short"));
    }

    let n = (data[0].leading_ones() as usize) + 1;

    if n > 5 {
        return Err(err_msg("Number too big"));
    }

    if data.len() < n  {
        return Err(err_msg("Not enough bytes"));
    }

    let mut buf = [0u8; 8];

    buf[(8 - n)..].copy_from_slice(&data[0..n]);

    let mut v = u64::from_be_bytes(buf);

    let num_valid_bits = (n * 8) - n;
    let mask = (1 << (num_valid_bits + 1)) - 1;
    v &= mask;

    Ok((v as usize, &data[n..]))
}


fn serialize_word(data: &[u8], out: &mut Vec<u8>) {
    serialize_length(data.len(), out);
    out.extend_from_slice(data);
}



