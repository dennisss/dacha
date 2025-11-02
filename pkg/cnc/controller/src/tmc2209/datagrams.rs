use common::array_ref;
use common::errors::*;
use crypto::checksum::crc8::crc8_atm;

const SYNC: u8 = 0b101;


pub fn create_tmc2209_write_request(node_addr: u8, register_addr: u8, data: u32) -> [u8; 8] {
    assert!(node_addr <= 3);
    assert!((register_addr & (1 << 7)) == 0);

    let data_bytes = data.to_be_bytes();

    let mut datagram = [
        SYNC,
        node_addr,
        register_addr | (1 << 7),
        data_bytes[0],
        data_bytes[1],
        data_bytes[2],
        data_bytes[3],
        0,
    ];

    datagram[7] = crc8_atm(&datagram[0..7]);

    datagram
}

pub fn create_tmc2209_read_request(node_addr: u8, register_addr: u8) -> [u8; 4] {
    assert!(node_addr <= 3);
    assert!((register_addr & (1 << 7)) == 0);

    let mut datagram = [
        SYNC,
        node_addr,
        register_addr,
        0
    ];

    
    datagram[3] = crc8_atm(&datagram[0..3]);

    datagram
}

pub fn parse_tmc2209_read_reply(datagram: &[u8; 8], expected_register_addr: u8) -> Result<u32> {
    if datagram[0] != SYNC {
        return Err(format_err!("Bad sync byte: {}", datagram[0]));
    }

    if datagram[1] != 0xFF { // master address
        return Err(format_err!("Bad address byte: {}", datagram[1]));
    }

    if datagram[2] != expected_register_addr {
        return Err(format_err!("Bad register address byte: {} (expected {})", datagram[2], expected_register_addr));
    }

    let data = u32::from_be_bytes(*array_ref![datagram, 3, 4]);

    if datagram[7] != crc8_atm(&datagram[0..7]) {
        return Err(format_err!("Bad CRC byte: {}", datagram[7]));;
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmc2209_crc_test() {

        let test_cases: &'static [(&'static [u8], u8)] = &[
            (b"", 0),
            (b"\x00", 0),
            (b"\x00\x00", 0),
            (b"\xFF\x00", 215),
            (b"\x00\xFF", 243),
            (b"\x12\x34\x45", 226),
            (b"\x01", 137),
            (b"\x02", 199),
        ];

        for (data, expected_value) in test_cases.iter().cloned() {
            assert_eq!(crc8_atm(data), expected_value);
        }

    }

}

