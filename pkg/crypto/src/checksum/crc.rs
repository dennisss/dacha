// TODO: Move crypto package.

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::convert::TryInto;

use crate::hasher::*;

const CRC32_TABLE: [u32; 256] = [
    0x00000000, 0x77073096, 0xee0e612c, 0x990951ba, 0x076dc419, 0x706af48f, 0xe963a535, 0x9e6495a3,
    0x0edb8832, 0x79dcb8a4, 0xe0d5e91e, 0x97d2d988, 0x09b64c2b, 0x7eb17cbd, 0xe7b82d07, 0x90bf1d91,
    0x1db71064, 0x6ab020f2, 0xf3b97148, 0x84be41de, 0x1adad47d, 0x6ddde4eb, 0xf4d4b551, 0x83d385c7,
    0x136c9856, 0x646ba8c0, 0xfd62f97a, 0x8a65c9ec, 0x14015c4f, 0x63066cd9, 0xfa0f3d63, 0x8d080df5,
    0x3b6e20c8, 0x4c69105e, 0xd56041e4, 0xa2677172, 0x3c03e4d1, 0x4b04d447, 0xd20d85fd, 0xa50ab56b,
    0x35b5a8fa, 0x42b2986c, 0xdbbbc9d6, 0xacbcf940, 0x32d86ce3, 0x45df5c75, 0xdcd60dcf, 0xabd13d59,
    0x26d930ac, 0x51de003a, 0xc8d75180, 0xbfd06116, 0x21b4f4b5, 0x56b3c423, 0xcfba9599, 0xb8bda50f,
    0x2802b89e, 0x5f058808, 0xc60cd9b2, 0xb10be924, 0x2f6f7c87, 0x58684c11, 0xc1611dab, 0xb6662d3d,
    0x76dc4190, 0x01db7106, 0x98d220bc, 0xefd5102a, 0x71b18589, 0x06b6b51f, 0x9fbfe4a5, 0xe8b8d433,
    0x7807c9a2, 0x0f00f934, 0x9609a88e, 0xe10e9818, 0x7f6a0dbb, 0x086d3d2d, 0x91646c97, 0xe6635c01,
    0x6b6b51f4, 0x1c6c6162, 0x856530d8, 0xf262004e, 0x6c0695ed, 0x1b01a57b, 0x8208f4c1, 0xf50fc457,
    0x65b0d9c6, 0x12b7e950, 0x8bbeb8ea, 0xfcb9887c, 0x62dd1ddf, 0x15da2d49, 0x8cd37cf3, 0xfbd44c65,
    0x4db26158, 0x3ab551ce, 0xa3bc0074, 0xd4bb30e2, 0x4adfa541, 0x3dd895d7, 0xa4d1c46d, 0xd3d6f4fb,
    0x4369e96a, 0x346ed9fc, 0xad678846, 0xda60b8d0, 0x44042d73, 0x33031de5, 0xaa0a4c5f, 0xdd0d7cc9,
    0x5005713c, 0x270241aa, 0xbe0b1010, 0xc90c2086, 0x5768b525, 0x206f85b3, 0xb966d409, 0xce61e49f,
    0x5edef90e, 0x29d9c998, 0xb0d09822, 0xc7d7a8b4, 0x59b33d17, 0x2eb40d81, 0xb7bd5c3b, 0xc0ba6cad,
    0xedb88320, 0x9abfb3b6, 0x03b6e20c, 0x74b1d29a, 0xead54739, 0x9dd277af, 0x04db2615, 0x73dc1683,
    0xe3630b12, 0x94643b84, 0x0d6d6a3e, 0x7a6a5aa8, 0xe40ecf0b, 0x9309ff9d, 0x0a00ae27, 0x7d079eb1,
    0xf00f9344, 0x8708a3d2, 0x1e01f268, 0x6906c2fe, 0xf762575d, 0x806567cb, 0x196c3671, 0x6e6b06e7,
    0xfed41b76, 0x89d32be0, 0x10da7a5a, 0x67dd4acc, 0xf9b9df6f, 0x8ebeeff9, 0x17b7be43, 0x60b08ed5,
    0xd6d6a3e8, 0xa1d1937e, 0x38d8c2c4, 0x4fdff252, 0xd1bb67f1, 0xa6bc5767, 0x3fb506dd, 0x48b2364b,
    0xd80d2bda, 0xaf0a1b4c, 0x36034af6, 0x41047a60, 0xdf60efc3, 0xa867df55, 0x316e8eef, 0x4669be79,
    0xcb61b38c, 0xbc66831a, 0x256fd2a0, 0x5268e236, 0xcc0c7795, 0xbb0b4703, 0x220216b9, 0x5505262f,
    0xc5ba3bbe, 0xb2bd0b28, 0x2bb45a92, 0x5cb36a04, 0xc2d7ffa7, 0xb5d0cf31, 0x2cd99e8b, 0x5bdeae1d,
    0x9b64c2b0, 0xec63f226, 0x756aa39c, 0x026d930a, 0x9c0906a9, 0xeb0e363f, 0x72076785, 0x05005713,
    0x95bf4a82, 0xe2b87a14, 0x7bb12bae, 0x0cb61b38, 0x92d28e9b, 0xe5d5be0d, 0x7cdcefb7, 0x0bdbdf21,
    0x86d3d2d4, 0xf1d4e242, 0x68ddb3f8, 0x1fda836e, 0x81be16cd, 0xf6b9265b, 0x6fb077e1, 0x18b74777,
    0x88085ae6, 0xff0f6a70, 0x66063bca, 0x11010b5c, 0x8f659eff, 0xf862ae69, 0x616bffd3, 0x166ccf45,
    0xa00ae278, 0xd70dd2ee, 0x4e048354, 0x3903b3c2, 0xa7672661, 0xd06016f7, 0x4969474d, 0x3e6e77db,
    0xaed16a4a, 0xd9d65adc, 0x40df0b66, 0x37d83bf0, 0xa9bcae53, 0xdebb9ec5, 0x47b2cf7f, 0x30b5ffe9,
    0xbdbdf21c, 0xcabac28a, 0x53b39330, 0x24b4a3a6, 0xbad03605, 0xcdd70693, 0x54de5729, 0x23d967bf,
    0xb3667a2e, 0xc4614ab8, 0x5d681b02, 0x2a6f2b94, 0xb40bbe37, 0xc30c8ea1, 0x5a05df1b, 0x2d02ef8d,
];

const CRC32C_TABLE: [u32; 256] = [
    0x00000000, 0xF26B8303, 0xE13B70F7, 0x1350F3F4, 0xC79A971F, 0x35F1141C, 0x26A1E7E8, 0xD4CA64EB,
    0x8AD958CF, 0x78B2DBCC, 0x6BE22838, 0x9989AB3B, 0x4D43CFD0, 0xBF284CD3, 0xAC78BF27, 0x5E133C24,
    0x105EC76F, 0xE235446C, 0xF165B798, 0x030E349B, 0xD7C45070, 0x25AFD373, 0x36FF2087, 0xC494A384,
    0x9A879FA0, 0x68EC1CA3, 0x7BBCEF57, 0x89D76C54, 0x5D1D08BF, 0xAF768BBC, 0xBC267848, 0x4E4DFB4B,
    0x20BD8EDE, 0xD2D60DDD, 0xC186FE29, 0x33ED7D2A, 0xE72719C1, 0x154C9AC2, 0x061C6936, 0xF477EA35,
    0xAA64D611, 0x580F5512, 0x4B5FA6E6, 0xB93425E5, 0x6DFE410E, 0x9F95C20D, 0x8CC531F9, 0x7EAEB2FA,
    0x30E349B1, 0xC288CAB2, 0xD1D83946, 0x23B3BA45, 0xF779DEAE, 0x05125DAD, 0x1642AE59, 0xE4292D5A,
    0xBA3A117E, 0x4851927D, 0x5B016189, 0xA96AE28A, 0x7DA08661, 0x8FCB0562, 0x9C9BF696, 0x6EF07595,
    0x417B1DBC, 0xB3109EBF, 0xA0406D4B, 0x522BEE48, 0x86E18AA3, 0x748A09A0, 0x67DAFA54, 0x95B17957,
    0xCBA24573, 0x39C9C670, 0x2A993584, 0xD8F2B687, 0x0C38D26C, 0xFE53516F, 0xED03A29B, 0x1F682198,
    0x5125DAD3, 0xA34E59D0, 0xB01EAA24, 0x42752927, 0x96BF4DCC, 0x64D4CECF, 0x77843D3B, 0x85EFBE38,
    0xDBFC821C, 0x2997011F, 0x3AC7F2EB, 0xC8AC71E8, 0x1C661503, 0xEE0D9600, 0xFD5D65F4, 0x0F36E6F7,
    0x61C69362, 0x93AD1061, 0x80FDE395, 0x72966096, 0xA65C047D, 0x5437877E, 0x4767748A, 0xB50CF789,
    0xEB1FCBAD, 0x197448AE, 0x0A24BB5A, 0xF84F3859, 0x2C855CB2, 0xDEEEDFB1, 0xCDBE2C45, 0x3FD5AF46,
    0x7198540D, 0x83F3D70E, 0x90A324FA, 0x62C8A7F9, 0xB602C312, 0x44694011, 0x5739B3E5, 0xA55230E6,
    0xFB410CC2, 0x092A8FC1, 0x1A7A7C35, 0xE811FF36, 0x3CDB9BDD, 0xCEB018DE, 0xDDE0EB2A, 0x2F8B6829,
    0x82F63B78, 0x709DB87B, 0x63CD4B8F, 0x91A6C88C, 0x456CAC67, 0xB7072F64, 0xA457DC90, 0x563C5F93,
    0x082F63B7, 0xFA44E0B4, 0xE9141340, 0x1B7F9043, 0xCFB5F4A8, 0x3DDE77AB, 0x2E8E845F, 0xDCE5075C,
    0x92A8FC17, 0x60C37F14, 0x73938CE0, 0x81F80FE3, 0x55326B08, 0xA759E80B, 0xB4091BFF, 0x466298FC,
    0x1871A4D8, 0xEA1A27DB, 0xF94AD42F, 0x0B21572C, 0xDFEB33C7, 0x2D80B0C4, 0x3ED04330, 0xCCBBC033,
    0xA24BB5A6, 0x502036A5, 0x4370C551, 0xB11B4652, 0x65D122B9, 0x97BAA1BA, 0x84EA524E, 0x7681D14D,
    0x2892ED69, 0xDAF96E6A, 0xC9A99D9E, 0x3BC21E9D, 0xEF087A76, 0x1D63F975, 0x0E330A81, 0xFC588982,
    0xB21572C9, 0x407EF1CA, 0x532E023E, 0xA145813D, 0x758FE5D6, 0x87E466D5, 0x94B49521, 0x66DF1622,
    0x38CC2A06, 0xCAA7A905, 0xD9F75AF1, 0x2B9CD9F2, 0xFF56BD19, 0x0D3D3E1A, 0x1E6DCDEE, 0xEC064EED,
    0xC38D26C4, 0x31E6A5C7, 0x22B65633, 0xD0DDD530, 0x0417B1DB, 0xF67C32D8, 0xE52CC12C, 0x1747422F,
    0x49547E0B, 0xBB3FFD08, 0xA86F0EFC, 0x5A048DFF, 0x8ECEE914, 0x7CA56A17, 0x6FF599E3, 0x9D9E1AE0,
    0xD3D3E1AB, 0x21B862A8, 0x32E8915C, 0xC083125F, 0x144976B4, 0xE622F5B7, 0xF5720643, 0x07198540,
    0x590AB964, 0xAB613A67, 0xB831C993, 0x4A5A4A90, 0x9E902E7B, 0x6CFBAD78, 0x7FAB5E8C, 0x8DC0DD8F,
    0xE330A81A, 0x115B2B19, 0x020BD8ED, 0xF0605BEE, 0x24AA3F05, 0xD6C1BC06, 0xC5914FF2, 0x37FACCF1,
    0x69E9F0D5, 0x9B8273D6, 0x88D28022, 0x7AB90321, 0xAE7367CA, 0x5C18E4C9, 0x4F48173D, 0xBD23943E,
    0xF36E6F75, 0x0105EC76, 0x12551F82, 0xE03E9C81, 0x34F4F86A, 0xC69F7B69, 0xD5CF889D, 0x27A40B9E,
    0x79B737BA, 0x8BDCB4B9, 0x988C474D, 0x6AE7C44E, 0xBE2DA0A5, 0x4C4623A6, 0x5F16D052, 0xAD7D5351,
];

// Bit representaion of the polynomials minus 2^32
const CRC32_POLYNOMIAL: u32 = 0x04C11DB7;
const CRC32_POLYNOMIAL_REVERSED: u32 = 0xEDB88320;
const CRC32C_POLYNOMIAL: u32 = 0x1edc6f41;

// 0x4c11db7

/// NOTE: The hash is returned in big endian.
#[derive(Clone)]
pub struct CRC32Hasher {
    val: u32,
}

impl CRC32Hasher {
    pub fn new() -> Self {
        Self { val: !0 }
    }

    pub fn finish_u32(&self) -> u32 {
        !self.val
    }

    // TODO: Speed up with CLMUL?

    fn update_bytewise(&mut self, data: &[u8]) {
        /*
        for byte in data.iter().cloned() {
            self.val ^= (byte as u32) << 24;

            for _ in 0..8 {
                let overflow = (self.val & (0b1 << 31)) != 0;
                self.val <<= 1;
                if overflow {
                    self.val ^= CRC32_POLYNOMIAL;
                }
            }
        }
        */

        for byte in data.iter().cloned() {
            self.val ^= byte as u32;

            for _ in 0..8 {
                let overflow = (self.val & 0b1) != 0;
                self.val >>= 1;

                if overflow {
                    self.val ^= CRC32_POLYNOMIAL_REVERSED;
                }
            }
        }
    }

    fn update_lut(&mut self, data: &[u8]) {
        for byte in data.iter().cloned() {
            let idx = (self.val as u8) ^ byte;
            self.val = (self.val >> 8) ^ CRC32_TABLE[idx as usize];
        }
    }
}

impl Hasher for CRC32Hasher {
    fn block_size(&self) -> usize {
        1
    }

    fn output_size(&self) -> usize {
        4
    }

    fn update(&mut self, data: &[u8]) {
        self.update_lut(data);
    }

    #[cfg(feature = "alloc")]
    fn finish(&self) -> Vec<u8> {
        self.finish_u32().to_be_bytes().to_vec()
    }

    #[cfg(feature = "alloc")]
    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}

/// SSE4.2 CRC-32C computation.
/// NOTE: The hash is returned in big endian.
#[derive(Clone)]
pub struct CRC32CHasher {
    val: u32,
}

impl CRC32CHasher {
    pub fn new() -> CRC32CHasher {
        CRC32CHasher { val: !0 }
    }

    pub fn raw_value(&self) -> u32 {
        self.val
    }

    pub fn finish_u32(&self) -> u32 {
        !self.val
    }

    pub fn masked(&self) -> u32 {
        self.finish_u32().rotate_right(15).wrapping_add(0xa282ead8)
    }

    fn update_lut(&mut self, data: &[u8]) {
        for byte in data {
            let idx = (self.val as u8) ^ *byte;
            self.val = (self.val >> 8) ^ CRC32C_TABLE[idx as usize];
        }
    }

    #[cfg(target_feature = "sse4.2")]
    fn update_sse42(&mut self, data: &[u8]) {
        // Calculate in 64bit chunks.
        let inc = core::mem::size_of::<u64>();
        let n = data.len() / inc;
        for i in 0..n {
            let pos = i * inc;
            let v = u64::from_le_bytes(data[pos..(pos + inc)].try_into().unwrap());
            unsafe {
                self.val = core::arch::x86_64::_mm_crc32_u64(self.val as u64, v) as u32;
            }
        }

        // Compute rest as individual bytes.
        for i in &data[(n * inc)..] {
            unsafe {
                self.val = core::arch::x86_64::_mm_crc32_u8(self.val, *i);
            }
        }
    }

    #[cfg(all(target_arch = "arm", target_feature = "crc32"))]
    fn update_arm_crc32(&mut self, data: &[u8]) {
        // Calculate 32-bit chunks.
        let inc = core::mem::size_of::<u32>();
        let n = data.len() / inc;
        for i in 0..n {
            let pos = i * inc;
            let v = u32::from_le_bytes(data[pos..(pos + inc)].try_into().unwrap());
            unsafe {
                asm!(
                    "crc32cw {0}, {0}, {1}",
                    inout(reg) self.val,
                    in(reg) v
                );
            }
        }

        // Compute rest as individual bytes.
        for i in &data[(n * inc)..] {
            unsafe { asm!("crc32cb {0}, {0}, {1}") }

            // unsafe {
            //     self.val = core::arch::x86_64::_mm_crc32_u8(self.val, *i);
            // }
        }
    }
}

impl Hasher for CRC32CHasher {
    fn block_size(&self) -> usize {
        1
    }

    fn output_size(&self) -> usize {
        4
    }

    #[cfg(target_feature = "sse4.2")]
    fn update(&mut self, data: &[u8]) {
        self.update_sse42(data);
    }

    #[cfg(not(target_feature = "sse4.2"))]
    fn update(&mut self, data: &[u8]) {
        self.update_lut(data);
    }

    // TODO: Ideally find a way to directly return the inner.
    #[cfg(feature = "alloc")]
    fn finish(&self) -> Vec<u8> {
        self.finish_u32().to_be_bytes().to_vec()
    }

    #[cfg(feature = "alloc")]
    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}

/// Performs the fastest CRC32-C hashing operation possible to convert one u32
/// to another randomly distributed u32.
///
/// WARNING: The value of this may not be stable across machines or binaries.
#[cfg(target_feature = "sse4.2")]
#[inline(always)]
pub fn crc32c_raw_oneshot(data: u32) -> u32 {
    unsafe { core::arch::x86_64::_mm_crc32_u32(0, data) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_test() {
        let crc = |input: &[u8]| {
            let mut hasher = CRC32Hasher::new();
            hasher.update(input);
            hasher.finish_u32()
        };

        assert_eq!(crc(&[0]), 0xd202ef8d);

        assert_eq!(crc(b"a"), 0xE8B7BE43);
        assert_eq!(crc(b"ab"), 0x9E83486D);
        assert_eq!(crc(b"hello world"), 0xd4a1185);
        assert_eq!(crc(b"123456789"), 0xCBF43926)
    }

    #[test]
    fn crc32c_test() {
        // See https://tools.ietf.org/html/rfc3720#appendix-B.4

        let crc = |input: &[u8]| {
            let mut hasher = CRC32CHasher::new();
            hasher.update(input);
            hasher.finish_u32()
        };

        let allzero = [0u8; 32];
        assert_eq!(crc(&allzero), 0x8a9136aa);

        let allone = [0xffu8; 32];
        assert_eq!(crc(&allone), 0x62a8ab43);

        let scsi_read: &'static [u8] = &[
            0x01, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x14,
            0x00, 0x00, 0x00, 0x18, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(crc(scsi_read), 0xd9963a56);
    }
}
