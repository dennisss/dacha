use crate::bits::BitVector;

pub struct BitSet {
    data: BitVector,
}

impl BitSet {
    pub fn new(max_value: usize) -> Self {
        let data = BitVector::from_raw_vec(vec![0u8; crate::ceil_div(max_value, 8)]);
        Self { data }
    }

    pub fn insert(&mut self, value: usize) {
        self.data.set(value, 1);
    }

    pub fn contains(&self, value: usize) -> bool {
        self.data.get(value).unwrap() == 1
    }

    pub fn clear(&mut self) {
        self.data.set_all_zero();
    }
}
