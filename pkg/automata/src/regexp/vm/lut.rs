/// An 8-bit set used to determine if a byte should be matched or not.
#[derive(Default)]
pub struct LookupTable {
    values: [u32; 256 / 32],
}

impl LookupTable {
    pub fn contains(&self, byte: u8) -> bool {
        todo!()
    }

    pub fn insert(&mut self, byte: u8) {
        //
    }
}
