/*
Neopixel protocol:
- Reset by staying low.
- 24 bits
    - Start high
        - 0.2 to 0.4us for a '0' code
            - Followed by at least 0.8us of low
        - 0.58 to 1.0us for a '1')
            - Followed by at least 0.2us of low
    - 80us of low is a reset

At 8MHz SPI, one bit is 0.125 us
- Use 2 bits for low,
- Use 6 bits for high
- Represent as two bytes with the second byte being always high
- so a 24-bit color requires 48 bytes to transfer

- Easiest to do with with SPI
*/

// NOTE: The color will be transfered MSB first.
// The ordering of channels should be GRB
fn expand_color(color: u32) -> [u8; 48] {
    let mut buf = [0u8; 48];
    for i in 0..24 {
        let bit = (color >> (24 - i)) & 1;
        buf[2 * i] = {
            if bit != 0 {
                0b11111100
            } else {
                0b11000000
            }
        };
    }

    buf
}
