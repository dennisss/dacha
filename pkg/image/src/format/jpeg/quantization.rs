use crate::format::jpeg::constants::BLOCK_SIZE;

/// Base luminance quantization table used as the default table for quality 50
/// (out of 100).
///
/// From ITU T.81, Table K.1.
const BASE_LUMINANCE_QUANTIZATION_TABLE: [u8; BLOCK_SIZE] = [
    16, 11, 10, 16, 124, 140, 151, 161, 12, 12, 14, 19, 126, 158, 160, 155, 14, 13, 16, 24, 140,
    157, 169, 156, 14, 17, 22, 29, 151, 187, 180, 162, 18, 22, 37, 56, 168, 109, 103, 177, 24, 35,
    55, 64, 181, 104, 113, 192, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103,
    199,
];

/// Base chrominance quantization table used as the default table for quality 50
/// (out of 100).
///
/// From ITU T.81, Table K.2.
const BASE_CHROMINANCE_QUANTIZATION_TABLE: [u8; BLOCK_SIZE] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

/// Creates a suitable quantization table for quantizing an image.
///
/// Arguments:
///   quality: Integer from 0 to 100
///
/// Returns: Both the luminance and chrominance tables to use.
pub fn create_quantization_tables(quality: usize) -> ([u8; BLOCK_SIZE], [u8; BLOCK_SIZE]) {
    let mut lum = BASE_LUMINANCE_QUANTIZATION_TABLE.clone();
    scale_quantization_table(&mut lum, quality);

    let mut chrom = BASE_CHROMINANCE_QUANTIZATION_TABLE.clone();
    scale_quantization_table(&mut chrom, quality);

    (lum, chrom)
}

fn scale_quantization_table(base_table: &mut [u8; BLOCK_SIZE], quality: usize) {
    let s_q = if quality < 50 {
        5000 / quality
    } else {
        200 - 2 * quality
    };

    for v in base_table.iter_mut() {
        let new_v = ((*v as usize) * s_q + 50) / 100;
        *v = (new_v as u8).min(255).max(1);
    }
}

pub fn quantize_block(table: &[u8; BLOCK_SIZE], block: &mut [i16; BLOCK_SIZE]) {
    for (v, coeff) in block.iter_mut().zip(table) {
        *v /= *coeff as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::create_quantization_tables;

    #[test]
    fn lossless_quantization() {
        println!("{:?}", create_quantization_tables(50));
    }
}
