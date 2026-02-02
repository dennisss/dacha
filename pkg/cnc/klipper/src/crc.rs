// Written by Google Gemini
// https://gemini.google.com/app/1819ea07a42f1c48

pub fn klipper_crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;

    for &byte in data {
        let mut x = (byte as u16) ^ (crc & 0xFF);
        x ^= (x & 0x0F) << 4;
        crc = ((x << 8) | (crc >> 8))
              ^ (x >> 4)
              ^ (x << 3);
    }

    crc
}
