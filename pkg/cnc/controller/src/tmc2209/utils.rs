
pub trait Register {
    fn addr() -> u8;

    fn from_raw(value: u32) -> Self;

    fn to_raw(&self) -> u32;
}
