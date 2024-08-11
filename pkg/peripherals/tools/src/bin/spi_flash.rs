// Utility for reading/writing from SPI flash chips.

#[macro_use]
extern crate macros;

use common::errors::*;
use peripherals::spi::*;

/*
SPI mode 0
CS active low.

We want to read a `JD2336 25D20ATIG` chip

- Something like this https://www.byte-semi.com/download/SPI_NOR_Flash/BY25D20AS.pdf

- 4KB sector (000FFFh bytes)

- Size is 03FFFFh + 1 bytes (64 sectors)

*/

const READ_COMMAND_ID: u8 = 0x03;
const REMS_COMMAND_ID: u8 = 0x90;
const RDID_COMMAND_ID: u8 = 0x9F;

const EN4B_COMMAND_ID: u8 = 0xB7;

#[derive(Args)]
struct Args {}

#[derive(Args)]
enum Command {}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut spi = SPIDevice::open("/dev/spidev0.0")?;

    const DUMMY: u8 = 0;

    {
        let send = &[RDID_COMMAND_ID];

        // Response is [manufacter id, memory type, memory density]
        let mut receive = [0u8; 3];
        spi.transfer(send, &mut receive)?;

        println!("0x9F: {:02x?}", receive);

        // assert_eq!(&receive[..], &[MACRONIX_MANUFACTURER_ID, 0x20, 0x19]);
    }

    // TODO: Check this.
    {
        // Send the manufacter id at index 0 first followed by the device id.
        let addr = 0;

        let send = &[REMS_COMMAND_ID, DUMMY, DUMMY, addr];

        // Response is the manufacturer id and device id
        let mut receive = [0u8; 2];
        spi.transfer(send, &mut receive)?;

        println!("0x90: {:02x?}", receive);

        // assert_eq!(&receive[..], &[MACRONIX_MANUFACTURER_ID, 0x18]);
    }

    let mut buf = vec![0u8; 262144];

    const PAGE_SIZE: usize = 4096;

    for i in 0..64 {
        let addr = i * PAGE_SIZE;

        let addrb = (addr as u32).to_be_bytes();

        let mut send = [0x03, addrb[0], addrb[1], addrb[2]];

        let receive = &mut buf[addr..(addr + PAGE_SIZE)];
        spi.transfer(&send, receive)?;
    }

    file::write("flash.bin", &buf).await?;

    Ok(())
}
