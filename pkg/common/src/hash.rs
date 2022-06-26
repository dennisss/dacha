/// Very simpler Hasher which just sums up the values passed to it.
/// Meant for use-cases such as hashing well defined enum values (where DOS is
/// not a concern).
pub struct SumHasher {
    total: u64,
}

impl core::hash::Hasher for SumHasher {
    fn finish(&self) -> u64 {
        self.total
    }

    fn write(&mut self, bytes: &[u8]) {
        todo!()
    }

    fn write_u64(&mut self, i: u64) {
        self.total += i;
    }

    fn write_u32(&mut self, i: u32) {
        self.total += i as u64;
    }
}

#[derive(Default)]
pub struct SumHasherBuilder {}

impl core::hash::BuildHasher for SumHasherBuilder {
    type Hasher = SumHasher;

    fn build_hasher(&self) -> Self::Hasher {
        SumHasher { total: 0 }
    }
}
