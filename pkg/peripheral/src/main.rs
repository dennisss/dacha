#[macro_use]
extern crate common;
extern crate peripheral;

use common::errors::*;
use peripheral::bmp388::*;
use peripheral::ddc::DDCDevice;
use peripheral::ds3231::*;
use peripheral::flash::FlashChip;
use peripheral::sgp30::*;
use peripheral::spi::*;

/*
cross build --target=armv7-unknown-linux-gnueabihf --package peripheral --release
scp target/armv7-unknown-linux-gnueabihf/release/peripheral pi@10.1.0.44:~/

scp pi@10.1.0.44:~/lepton.pgm .
*/

/*
When not waiting:
- Error: Os { code: 121, kind: Uncategorized, message: "Remote I/O error" }

*/

fn main() -> Result<()> {
    println!("OPEN");

    let mut lepton = peripheral::lepton::Lepton::open("/dev/spidev0.0")?;

    let frame = lepton.read_frame()?;
    println!("Got a frame!");

    let min = 29300;
    let max = 30000;

    let mut pgm = String::new();
    pgm.push_str("P2\n");
    pgm.push_str("160 120\n");
    pgm.push_str(&format!("{}\n", max - min));

    for i in (0..frame.len()).step_by(2) {
        let mut num = u16::from_be_bytes(*array_ref![frame, i, 2]);

        num = std::cmp::max(std::cmp::min(num, max), min);

        num = num - min;

        pgm.push_str(&format!("{}\n", num));
    }

    std::fs::write("/home/pi/lepton.pgm", &pgm)?;

    /*
    let mut tft = peripheral::tft::SparkFun18TFT::open("/dev/spidev0.0", 6, 12)?;

    let mut buf = vec![0u8; tft.rows() * tft.cols() * tft.bytes_per_pixel()];

    let mut start_line = 0;
    loop {
        let mut i = start_line * tft.cols() * tft.bytes_per_pixel();
        // let initial_i = i;

        let buf_len = buf.len();

        for _ in 0..(32 * tft.cols()) {
            buf[(i + 0) % buf_len] = 0xff;
            buf[(i + 1) % buf_len] = 0;
            buf[(i + 2) % buf_len] = 0;
            i += 3;
        }

        for _ in 0..(32 * tft.cols()) {
            buf[(i + 0) % buf_len] = 0;
            buf[(i + 1) % buf_len] = 0xff;
            buf[(i + 2) % buf_len] = 0;
            i += 3;
        }

        for _ in 0..(32 * tft.cols()) {
            buf[(i + 0) % buf_len] = 0;
            buf[(i + 1) % buf_len] = 0;
            buf[(i + 2) % buf_len] = 0xff;
            i += 3;
        }

        for _ in 0..(32 * tft.cols()) {
            buf[(i + 0) % buf_len] = 0xff;
            buf[(i + 1) % buf_len] = 0;
            buf[(i + 2) % buf_len] = 0xff;
            i += 3;
        }

        // assert_eq!(i % buf_len, initial_i);

        tft.draw_frame(&buf)?;

        start_line = (start_line + 1) % tft.rows();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    */

    /*
    let i2c = peripheral::i2c::I2CDevice::open("/dev/i2c-1")?;

    let mut dev = BMP388::open(i2c)?;

    let mut i = 0;
    loop {
        let m = dev.measure()?;

        if i % 20 == 0 {
            println!("{:?}", m);
        }

        // std::thread::sleep(std::time::Duration::from_secs(1));
        i += 1;
    }
    */

    /*
    let i2c = peripheral::i2c::I2CDevice::open("/dev/i2c-1")?;

    let mut dev = SGP30::open(i2c);

    let serial = dev.get_serial()?;
    println!("SERIAL {:?}", serial);

    dev.init_air_quality()?;

    let mut i = 0;
    loop {
        let quality = dev.measure_air_quality()?;
        println!("{:?}", quality);

        if i % 10 == 0 {
            let baseline = dev.get_baseline()?;
            println!("Baseline: {:?}", baseline);
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
        i += 1;
    }
    */

    /*
    let mut spi = SPIDevice::open("/dev/spidev0.0")?;
    let mut flash = FlashChip::open(spi)?;

    let buf = flash.read_all()?;

    std::fs::write("/home/pi/flash_dump2", &buf)?;
    */

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
