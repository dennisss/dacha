use common::errors::*;

pub const ANGLECOM: u16 = 0x3FFF;
pub const DIAAGC: u16 = 0x3FFC;
pub const ANGLEUNC: u16 = 0x3FFE;

pub fn create_as5047p_command(addr: u16, read: bool) -> [u8; 2] {
    assert!(addr & 0b0011_1111_1111_1111 == addr);

    let mut out = addr;

    if read {
        out |= 1 << 14;
    }

    // Even parity bit.
    if out.count_ones() % 2 != 0 {
        out |= 1 << 15;
    }

    out |= calculate_even_parity(out) << 15;
    
    out.to_be_bytes()
}

fn calculate_even_parity(mut value: u16) -> u16 {
    let mut parity = 0;
    while value > 0 {
        parity ^= value & 1;
        value >>= 1;
    }
    parity
}

pub fn parse_as5047p_data(data: &[u8; 2]) -> Result<u16> {
    let v = u16::from_be_bytes(*data);

    if v.count_ones() % 2 != 0 {
        return Err(err_msg("Bad response parity"));
    }

    if (v & (1 << 14)) != 0 {
        return Err(err_msg("Data indicates error"));
    }


    Ok(v & 0b0011_1111_1111_1111)
}
