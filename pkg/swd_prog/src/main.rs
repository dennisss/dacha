extern crate common;
extern crate rpi;

use std::thread::sleep;
use std::time::Duration;

use common::async_std::task;
use common::errors::*;

use rpi::gpio::*;

/*
"""
Send more than 50 SWCLKTCK cycles with SWDIOTMS=1. This ensures that both SWD and JTAG are in their reset states

Send the 16-bit JTAG-to-SWD select sequence on SWDIOTMS

Send more than 50 SWCLKTCK cycles with SWDIOTMS=1. This ensures that if SWJ-DP was already in SWD mode, before sending the select sequence, the SWD goes to line reset.

Perform a READID to validate that SWJ-DP has switched to SWD operation.

The 16-bit JTAG-to-SWD select sequence is defined to be 0b01111001 11100111, MSB first. This can be represented as 16'h79E7 if transmitted MSB first or 16'hE79E if transmitted LSB first.
"""
*/

/*
0b01111001 11100111

  10011110 11100111


"The data is set by the host during the rising edge and sampled by the DP during the falling edge of the SWDCLK signal.""

Start Bit: Always 1
APnDP: 0 for DP
RnW: Read is 1
ADDR[2:3] (2 bit address)
Odd parity over  (APnDP, RnW, A[2:3])
Stop bit: 0
Park: 1

Trn

ACK (3 bits)

If number of bits set to 1 is odd, set parity to 1


The NRF52:
- IO: Pull up
- CLK: Pull down
- https://infocenter.nordicsemi.com/index.jsp?topic=%2Fstruct_nrf52%2Fstruct%2Fnrf52832_ps.html
    - Clk must be at least 0.125MHz

*/

// 10KHz
const HALF_CYCLE_DURATION: Duration = Duration::from_nanos(1000000000 / 800000 / 2);

struct SWDPort {
    io_pin: GPIOPin,
    clk_pin: GPIOPin,
}

impl SWDPort {
    fn write_bits(&mut self, num_bits: usize, data: &[u8]) {
        self.io_pin.set_mode(Mode::Output);

        for i in 0..num_bits {
            let byte_i = i / 8;
            let bit_i = i % 8;

            let bit = (data[byte_i] >> bit_i) & 1 != 0;

            // Set data on rising edge.
            self.clk_pin.write(true);
            self.io_pin.write(bit);
            sleep(HALF_CYCLE_DURATION);

            // Allow time for the device to capture on falling edge.
            self.clk_pin.write(false);
            sleep(HALF_CYCLE_DURATION);
        }

        // Always end with the clock high
        self.clk_pin.write(true);
    }

    // Read data before it is low

    /*
    HIGH
    read
    LOW
    HIGH


    */

    fn read_bits(&mut self, num_bits: usize, data: &mut [u8]) {
        self.io_pin.set_mode(Mode::Input);

        for i in 0..num_bits {
            let byte_i = i / 8;
            let bit_i = i % 8;

            // Rising edge: wait for device to set a bit.
            self.clk_pin.write(true);
            sleep(HALF_CYCLE_DURATION);

            self.clk_pin.write(false);
            sleep(HALF_CYCLE_DURATION);

            let bit = if self.io_pin.read() { 1 } else { 0 };

            if bit_i == 0 {
                data[byte_i] = 0;
            }

            data[byte_i] |= bit << bit_i;
        }

        // Always end with the clock high
        self.clk_pin.write(true);
    }
}

fn run() -> Result<()> {
    let gpio = GPIO::open()?;

    let mut clk_pin = gpio.pin(25);
    clk_pin.set_mode(Mode::Output).write(true);

    let mut io_pin = gpio.pin(24);
    io_pin
        .set_mode(Mode::Output)
        .write(true)
        .set_resistor(Resistor::PullUp);

    println!("Start");
    sleep(Duration::from_secs(2));

    let mut swd = SWDPort { clk_pin, io_pin };

    swd.write_bits(50, &[0xffu8; 7]);
    swd.write_bits(16, &[0x9e, 0xe7]);
    swd.write_bits(50, &[0xffu8; 7]);

    // Read 0x00 from DP
    let request = 0b10100101;
    swd.write_bits(8, &[request]);
    swd.read_bits(1, &mut [0]); // Trn

    let mut ack = [0u8; 1];
    swd.read_bits(3, &mut ack[..]);

    let mut data = [0u8; 5];
    swd.read_bits(33, &mut data[..]);

    println!("{:?}", ack);
    println!("{:?}", data);

    println!("Done");
    sleep(Duration::from_secs(2));

    Ok(())
}

fn main() -> Result<()> {
    run()

    // task::block_on(run())
}
