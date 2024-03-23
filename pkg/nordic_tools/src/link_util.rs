use common::errors::*;
use crypto::random::SharedRng;

/// Generates a new random radio address.
///
/// This address must be:
/// - 4 bytes in length
/// - Have no zero bytes.
/// - Not contain the radio pre-amble bytes (0x55 or 0xAA)
pub async fn generate_radio_address() -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    buf.resize(4, 0);

    let rng = crypto::random::global_rng();
    for _ in 0..10 {
        rng.generate_bytes(&mut buf).await;

        if is_valid_address(&buf) {
            return Ok(buf);
        }
    }

    Err(err_msg("Failed to generate a new address"))
}

fn is_valid_address(addr: &[u8]) -> bool {
    let mut addr = u32::from_le_bytes(*array_ref![addr, 0, 4]);
    for i in 0..28 {
        let b = addr & 0xff;
        if b == 0 || b == 0xAA || b == 0x55 {
            return false;
        }
        addr >>= 1;
    }

    true
}
