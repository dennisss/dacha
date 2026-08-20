
pub type MaskingKey = [u8; 4];

enum_def_with_unknown!(OpCode u8 =>
    Continuation = 0,
    Text = 1,
    Binary = 2,
    Close = 8,
    Ping = 9,
    Pong = 0xA
);

#[derive(Debug)]
pub struct Frame<'a> {
    pub fin: bool,

    pub opcode: OpCode,

    pub mask: Option<MaskingKey>,

    /// Unmasked payload of the frame
    pub data: &'a [u8],
}

impl<'a> Frame<'a> {
    pub fn try_decode(buf: &'a mut [u8]) -> Option<(Frame<'a>, usize)> {

        let mut i = 0;
        if buf.len() < 2 {
            return None;
        }

        let fin = (buf[0] & (1 << 7)) != 0;
        let opcode = OpCode::from_value((buf[0] & 0b1111));
        let mask = (buf[1] & (1 << 7)) != 0;
        let payload_len = buf[1] & 0b0111_1111;

        let extended_payload_len_bytes = {
            if payload_len == 126 {
                2
            } else if payload_len == 127 {
                8
            } else {
                0
            }
        };

        let mask_bytes = {
            if mask { 4 } else { 0 }
        };

        i += 2;

        if buf.len() < i + extended_payload_len_bytes + mask_bytes {
            return None;
        }

        let payload_len = {
            let mut v = [0u8; 8];
            v[7] = payload_len;

            v[(8 - extended_payload_len_bytes)..].copy_from_slice(
                &buf[i..(i + extended_payload_len_bytes)]
            );

            u64::from_be_bytes(v) as usize
        };
        i += extended_payload_len_bytes;

        let mask = {
            if mask {
                Some(*array_ref![buf, i, 4])
            } else {
                None
            }
        };
        i += mask_bytes;

        if buf.len() < i + payload_len {
            return None;
        }

        let payload = &mut buf[i..(i + payload_len)];
        i += payload_len;

        if let Some(mask) = mask {
            apply_masking(&mask, payload);
        }

        Some((Frame { fin, opcode, mask, data: payload }, i))
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        assert!(self.mask.is_none());

        // TODO: Reserve size

        let fin = if self.fin { 1 } else { 0 };
        out.push(
            fin << 7 | self.opcode.to_value()
        );

        let mask = 0; // don't mask
        let mut extended_payload_len_bytes = 0;
        out.push(
            mask << 7 | (
                if self.data.len() <= 125 {
                    self.data.len() as u8
                } else if self.data.len() < (1 << 16) {
                    extended_payload_len_bytes = 2; 
                    126
                } else {
                    extended_payload_len_bytes = 8;
                    127
                }
            )
        );

        let extended_payload_len = (self.data.len() as u64).to_be_bytes();
        out.extend_from_slice(
            &extended_payload_len[(extended_payload_len.len() - extended_payload_len_bytes)..]
        );

        out.extend_from_slice(self.data);
    }
}

fn apply_masking(masking_key: &[u8; 4], data: &mut [u8]) {
    let mut chunks = data.chunks_exact_mut(4);
    for chunk in chunks.by_ref() {
        chunk[0] ^= masking_key[0];
        chunk[1] ^= masking_key[1];
        chunk[2] ^= masking_key[2];
        chunk[3] ^= masking_key[3];
    }
    
    let remainder = chunks.into_remainder();
    for (i, byte) in remainder.iter_mut().enumerate() {
        *byte ^= masking_key[i];
    }
}