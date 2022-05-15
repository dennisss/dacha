use crate::format::jpeg::constants::BLOCK_SIZE;

const ZIG_ZAG_SEQUENCE: &[u8; BLOCK_SIZE] = &[
    0, 1, 5, 6, 14, 15, 27, 28, //
    2, 4, 7, 13, 16, 26, 29, 42, //
    3, 8, 12, 17, 25, 30, 41, 43, //
    9, 11, 18, 24, 31, 40, 44, 53, //
    10, 19, 23, 32, 39, 45, 52, 54, //
    20, 22, 33, 38, 46, 51, 55, 60, //
    21, 34, 37, 47, 50, 56, 59, 61, //
    35, 36, 48, 49, 57, 58, 62, 63, //
];

pub fn apply_zigzag<T: Copy>(inputs: &[T], outputs: &mut [T]) {
    for i in 0..inputs.len() {
        outputs[ZIG_ZAG_SEQUENCE[i] as usize] = inputs[i];
    }
}

pub fn reverse_zigzag<T: Copy>(inputs: &[T], outputs: &mut [T]) {
    for i in 0..inputs.len() {
        outputs[i] = inputs[ZIG_ZAG_SEQUENCE[i] as usize];
    }
}
