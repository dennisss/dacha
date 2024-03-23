use core::arch::asm;

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

#[derive(Default, Clone)]
pub struct SumHasherBuilder {}

impl core::hash::BuildHasher for SumHasherBuilder {
    type Hasher = SumHasher;

    fn build_hasher(&self) -> Self::Hasher {
        SumHasher { total: 0 }
    }
}

pub struct FastHasher {
    total: u64,
}

impl core::hash::Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.total
    }

    fn write(&mut self, bytes: &[u8]) {
        todo!()
    }

    // Not setting to SSE4.2 do to the sheer incompetance of cargo to correctly
    // compile build scripts.
    // #[cfg(target_feature = "sse4.2")]
    #[cfg(target_arch = "x86_64")]
    fn write_u32(&mut self, i: u32) {
        self.total = unsafe { core::arch::x86_64::_mm_crc32_u32(self.total as u32, i) as u64 };
    }

    // #[cfg(target_feature = "sse4.2")]
    #[cfg(target_arch = "x86_64")]
    fn write_u64(&mut self, i: u64) {
        self.total = unsafe { core::arch::x86_64::_mm_crc32_u64(self.total, i) };
    }

    #[cfg(all(target_arch = "aarch64"))]
    fn write_u32(&mut self, i: u32) {
        unsafe {
            self.total = core::arch::aarch64::__crc32cw(self.total as u32, i) as u64;
        }

        // unsafe {
        //     asm!(
        //         "crc32cw {0}, {0}, {1}",
        //         inout(reg) self.total,
        //         in(reg) i
        //     );
        // }
    }

    #[cfg(all(target_arch = "aarch64"))]
    fn write_u64(&mut self, i: u64) {
        let upper = (i >> 32) as u32;
        let lower = i as u32;
        self.write_u32(upper);
        self.write_u32(lower);
    }

    #[cfg(target_pointer_width = "32")]
    fn write_usize(&mut self, i: usize) {
        self.write_u32(i as u32);
    }

    #[cfg(target_pointer_width = "64")]
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
}

#[derive(Default, Clone)]
pub struct FastHasherBuilder {}

impl core::hash::BuildHasher for FastHasherBuilder {
    type Hasher = FastHasher;

    fn build_hasher(&self) -> Self::Hasher {
        FastHasher { total: 0 }
    }
}
