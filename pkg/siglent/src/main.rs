#[macro_use]
extern crate common;
extern crate compression;

use common::errors::*;


/*
    At bottom of

*/

// Origina: 8F DE 02 3C
// Current: 70 21 02 C3
// Desired: 8F 21 02 C3

// Must change: 1, 3

fn main() -> Result<()> {
    let mut buf_full = std::fs::read(project_path!("ext/siglent/firmwares/SDS1004X_E_6.1.26.ADS"))?;

    // Skip the header.
    let mut buf = &mut buf_full[0x70..];

    buf.reverse();

    println!("Size: {}", buf.len());

    let mut i = 0;
    let mut spacing = 1;
    while i < buf.len() {
        buf[i] ^= 0xff;

        i += spacing;
        spacing += 1;
    }

    // XORing with 0xFF from 0x003B2FA5 until 0x00765F48
    //                           3b2fa4 -         765f49

    // println!("{:x} - {:x}", ((buf.len() + 0x70) / 2) - 36, buf.len());

    // TODO: Why plus 1?
    let center_i = (buf.len() / 2) + 1;
    for i in center_i..buf.len() {
        buf[i] ^= 0xff;
    }

    compression::zip::read_zip_file(&buf[0x34..])?;

    std::fs::write(project_path!("ext/siglent/decoded.bin"), buf)?;

    /*
        After reverse and xor:

        - 0x0: Checksum 4 bytes

    */

    Ok(())
}
