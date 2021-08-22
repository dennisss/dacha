extern crate common;
extern crate peripheral;

use common::errors::*;
use peripheral::ddc::DDCDevice;
use peripheral::ds3231::*;
use peripheral::flash::FlashChip;
use peripheral::spi::*;

/*
cross build --target=armv7-unknown-linux-gnueabihf --package peripheral
scp target/armv7-unknown-linux-gnueabihf/debug/peripheral pi@10.1.0.44:~/
*/

fn main() -> Result<()> {
    println!("OPEN");

    let mut spi = SPIDevice::open("/dev/spidev0.0")?;
    let mut flash = FlashChip::open(spi)?;

    let buf = flash.read_all()?;

    std::fs::write("/home/pi/flash_dump2", &buf)?;

    /*
    // On linux disconnecting gives the error:
    // Error: Os { code: 121, kind: Uncategorized, message: "Remote I/O error" }

    let i2c = peripheral::i2c::I2CDevice::open("/dev/i2c-12")?;

    let mut clock = DS3231::open(i2c);

    println!("Temp: {}", clock.read_temperature()?);
    clock.write_time(&DS3231Time::from_atomic_seconds(0))?;
    for i in 0..100 {
        let time = clock.read_time()?;
        println!("Time: {}", time.to_atomic_seconds());

        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    */

    // for i in 0..127 {
    //     if dev.test(i).is_ok() {
    //         println!("Good addr: {:2x}", i);
    //     }
    // }

    Ok(())
}
