#[macro_use]
extern crate common;

#[macro_use]
extern crate macros;

use std::time::Duration;

use common::errors::*;
use common::io::Writeable;
use peripherals::spi::*;
use file::LocalPathBuf;

/*
- https://www.analog.com/media/en/technical-documentation/data-sheets/adxl345.pdf
    - "The end of reading a data register is signified by the transition from Register 0x37 to
Register 0x38"


cargo run --bin builder -- build //pkg/peripherals/tools:adxl_read --config=//pkg/builder/config:rpi64


scp -i ~/.ssh/id_cluster built/pkg/peripherals/tools/adxl_read cluster-user@10.1.1.3:~/adxl_read

*/

const BW_RATE: u8 = 0x2C;
const POWER_CTL: u8 = 0x2D;
const DATA_FORMAT: u8 = 0x31;
const FIFO_CTL: u8 = 0x38;
const DATAX: u8 = 0x32;
const FIFO_STATUS: u8 = 0x39;

pub struct ADXL {
    spi: SPIDevice
}

impl ADXL {

    pub fn write_byte(&mut self, addr: u8, v: u8) -> Result<()> {
        let send = [
            (0 << 7) | // Write
            (0 << 6) | // Not multi-byte
            addr, // lower 6 is address
            v
        ];

        self.spi.transfer(&send, &mut [])?;
        Ok(())
    }

    pub fn read_byte(&mut self, addr: u8) -> Result<u8> {
        let send: [u8; 1] = [
            (1 << 7) | // Read
            (0 << 6) | // Not multi-byte
            addr, // lower 6 is address
        ];
        let mut recv = [0u8; 1];

        self.spi.transfer(&send, &mut recv)?;
        Ok(recv[0])
    }

    pub fn read_bytes(&mut self, addr: u8, out: &mut [u8]) -> Result<()> {
        let send: [u8; 1] = [
            (1 << 7) | // Read
            (1 << 6) | // Multi-byte
            addr, // lower 6 is address
        ];

        self.spi.transfer(&send, out)?;
        Ok(())
    }

    // pub fn read_data(&mut self) -> Result<[f32, f32, f32]> {


    // }

}

#[derive(Args)]
struct Args {
    output_path: LocalPathBuf
}


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut spi = SPIDevice::open("/dev/spidev0.0")?;
    spi.set_speed_hz(4_000_000)?;
    spi.set_mode(3)?;

    let mut adxl = ADXL { spi }; 

    let devid = adxl.read_byte(0)?;
    println!("Dev ID: {:x?}", devid);


    // LOW_POWER = off
    // Rate = 3200 Hz
    adxl.write_byte(BW_RATE, 0b1111)?;

    // Explicitly set to +/- 2g range
    // Also ensures SPI mode is 4-wire.
    adxl.write_byte(DATA_FORMAT, 0)?;

    // STREAM mode.
    adxl.write_byte(FIFO_CTL, 0b10 << 6)?;

    // Transition from standy mode (default on reset) to measure mode.
    adxl.write_byte(POWER_CTL, 1 << 3)?;

    let cancellation_token = executor::signals::new_shutdown_token();

    let mut output_file = file::LocalFile::open_with_options(
        args.output_path,
        file::LocalFileOpenOptions::new().write(true).create(true),
    )?;

    let mut output_buffer = vec![];

    const CHUNK_SIZE: usize = 4096;

    while !cancellation_token.is_cancelled().await {
        let status = adxl.read_byte(FIFO_STATUS)? & 0b111111;

        if status > 30 {
            eprintln!("OVERRUN");
        }

        for i in 0..status {
            let mut buf = [0u8; 6];
            adxl.read_bytes(DATAX, &mut buf)?;

            // TODO: Compress to 4 bytes per sample.
            output_buffer.extend_from_slice(&buf);

            /*
            const SCALE: f32 = 0.0039;
            let parts = [
                (i16::from_le_bytes(*array_ref![buf, 0, 2]) as f32) * SCALE,
                (i16::from_le_bytes(*array_ref![buf, 2, 2]) as f32) * SCALE,
                (i16::from_le_bytes(*array_ref![buf, 4, 2]) as f32) * SCALE,
            ];

            // println!("{:?}", parts);
            */

            // TODO: Make this part of the SPI transaction.
            std::thread::sleep(Duration::from_micros(5));
        }

        if output_buffer.len() >= CHUNK_SIZE {
            output_file.write_all(&output_buffer).await?;
            output_buffer.clear();
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    println!("Flushing...");
    output_file.write_all(&output_buffer).await?;
    output_buffer.clear();


    Ok(())
}
