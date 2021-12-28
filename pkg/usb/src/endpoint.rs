use common::errors::*;

pub const CONTROL_ENDPOINT: u8 = 0;

// NOTE: These don't
#[cfg(feature = "alloc")]
pub fn check_can_write_endpoint(address: u8) -> Result<()> {
    if address == CONTROL_ENDPOINT {
        return Err(err_msg("Invalid operation on control endpoint"));
    }

    if is_in_endpoint(address) {
        return Err(err_msg("Can not write to IN endpoint"));
    }

    Ok(())
}

#[cfg(feature = "alloc")]
pub fn check_can_read_endpoint(address: u8) -> Result<()> {
    if address == CONTROL_ENDPOINT {
        return Err(err_msg("Invalid operation on control endpoint"));
    }

    if !is_in_endpoint(address) {
        return Err(err_msg("Can not read to OUT endpoint"));
    }

    Ok(())
}

pub fn is_in_endpoint(address: u8) -> bool {
    address & (1 << 7) != 0
}
