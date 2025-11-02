
pub trait Register {
    fn addr() -> u8;

    fn from_raw(value: u32) -> Self;

    fn to_raw(&self) -> u32;
}

#[inline(always)]
pub fn get_bit_field(v: u32, shift: u32, width: u32) -> u32 {
    let mask = 1u32.checked_shl(width).unwrap_or(0).wrapping_sub(1);


    (v >> shift) & mask
}

#[inline(always)]
pub fn set_bit_field(v: u32, shift: u32, width: u32, field: u32) -> u32 {
    let mask = 1u32.checked_shl(width).unwrap_or(0).wrapping_sub(1);
    assert!((field & mask) == field); // verify field value not out of range.
 
    (v & !(mask << shift)) | (field << shift)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn get_bit_field_test() {
        assert_eq!(get_bit_field(0b00000001, 0, 1), 1);
        assert_eq!(get_bit_field(0b00010001, 0, 1), 1);
        assert_eq!(get_bit_field(0b00010001, 1, 1), 0);
        assert_eq!(get_bit_field(0b00010001, 4, 1), 1);
        assert_eq!(get_bit_field(0b00010001, 4, 2), 1);
        assert_eq!(get_bit_field(0b00010001, 3, 2), 0b10);
        assert_eq!(get_bit_field(0b00010001, 0, 32), 0b00010001);
        assert_eq!(get_bit_field(0b00010001, 0, 32), 0b00010001);
        assert_eq!(get_bit_field(0b00010001, 1, 31), 0b0001000);
    }

    #[test]
    fn set_bit_field_test() {
        assert_eq!(set_bit_field(0, 0, 1, 1), 1);
        assert_eq!(set_bit_field(0, 1, 1, 1), 0b10);
        assert_eq!(set_bit_field(0xA0, 0, 32, 0x0B), 0x0B);
        assert_eq!(set_bit_field(0b10, 5, 2, 0b11), 0b1100010);
        assert_eq!(set_bit_field(0b1100010, 5, 2, 0), 0b0000010);
        assert_eq!(set_bit_field(0b1100010, 5, 2, 0b10), 0b1000010);
    }

}