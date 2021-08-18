use crate::table::filter_block::*;

fn hash(data: &[u8], seed: u32) -> u32 {
    const m: u32 = 0xc6a4a793;
    const r: u32 = 24;
    let mut h: u32 = seed ^ m.wrapping_mul(data.len() as u32);

    let mut i = 0;
    while i + 4 <= data.len() {
        let w = u32::from_le_bytes(*array_ref![data, i, 4]);
        h = h.wrapping_add(w);
        h = h.wrapping_mul(m);
        h ^= h >> 16;
        i += 4;
    }

    let rem = data.len() - i;
    if rem == 3 {
        h = h.wrapping_add((data[i + 2] as u32) << 16);
    }
    if rem >= 2 {
        h = h.wrapping_add((data[i + 1] as u32) << 8);
    }
    if rem >= 1 {
        h = h.wrapping_add(data[i] as u32);
        h = h.wrapping_mul(m);
        h ^= h >> r;
    }

    h
}

fn bloom_hash(data: &[u8]) -> u32 {
    hash(data, 0xbc9f1d34)
}

struct Bit<T: AsRef<[u8]>> {
    data: T,
    pos: usize,
}

impl<T: AsRef<[u8]>> Bit<T> {
    pub fn at(data: T, pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn get(&self) -> bool {
        (self.data.as_ref()[self.pos / 8] & ((1 << (self.pos % 8)) as u8)) != 0
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> Bit<T> {
    pub fn set(&mut self) {
        self.data.as_mut()[self.pos / 8] |= (1 << (self.pos % 8)) as u8;
    }
}

pub struct BloomFilterPolicy {
    bits_per_key: usize,

    /// Number of bits marked in the filter per key.
    num_probes: usize,
}

impl Default for BloomFilterPolicy {
    /// NOTE: In LevelDB, the default is to have no filter.
    fn default() -> Self {
        Self::new(10)
    }
}

impl BloomFilterPolicy {
    pub fn new(bits_per_key: usize) -> Self {
        let mut num_probes = ((bits_per_key as f64) * 0.69) as usize;
        if num_probes < 1 {
            num_probes = 1;
        }
        if num_probes > 30 {
            num_probes = 30;
        }

        Self {
            bits_per_key,
            num_probes,
        }
    }
}

impl FilterPolicy for BloomFilterPolicy {
    fn name(&self) -> &'static str {
        "leveldb.BuiltinBloomFilter2"
    }

    fn create(&self, keys: Vec<&[u8]>, out: &mut Vec<u8>) {
        let mut nbits = keys.len() * self.bits_per_key;
        if nbits < 64 {
            nbits = 64;
        }

        let nbytes = common::ceil_div(nbits, 8);
        nbits = nbytes * 8;

        let start_offset = out.len();
        out.resize(start_offset + nbytes, 0);
        out.push(self.num_probes as u8);

        let mut filter = &mut out[start_offset..(start_offset + nbytes)];

        for key in keys {
            let mut h = bloom_hash(key);
            let delta = h.rotate_right(17);
            for _ in 0..self.num_probes {
                let bit = h % (nbits as u32);
                Bit::at(&mut filter, bit as usize).set();
                h += delta;
            }
        }
    }

    fn key_may_match(&self, key: &[u8], filter: &[u8]) -> bool {
        if filter.len() < 2 {
            return false;
        }

        let num_probes = *filter.last().unwrap() as usize;
        if num_probes > 30 {
            // Reserved for future use.
            return true;
        }

        let nbits = 8 * (filter.len() - 1);

        let mut h = bloom_hash(key);
        let delta = h.rotate_right(17);
        for _ in 0..num_probes {
            let bit = h % (nbits as u32);
            if !Bit::at(filter, bit as usize).get() {
                return false;
            }
            h += delta;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash() {
        let data1 = &[0x62];
        let data2 = &[0xc3, 0x97];
        let data3 = &[0xe2, 0x99, 0xa5];
        let data4 = &[0xe1, 0x80, 0xb9, 0x32];
        let data5 = &[
            0x01, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x14,
            0x00, 0x00, 0x00, 0x18, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        assert_eq!(hash(&[], 0xbc9f1d34), 0xbc9f1d34);
        assert_eq!(hash(data1, 0xbc9f1d34), 0xef1345c4);
        assert_eq!(hash(data2, 0xbc9f1d34), 0x5b663814);
        assert_eq!(hash(data3, 0xbc9f1d34), 0x323c078f);
        assert_eq!(hash(data4, 0xbc9f1d34), 0xed21633a);
        assert_eq!(hash(data5, 0x12345678), 0xf333dabb);
    }
}
