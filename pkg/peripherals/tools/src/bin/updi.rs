

/*
https://github.com/microchip-pic-avr-tools/pymcuprog/tree/main
https://github.com/pyserial/pyserial/blob/master/serial/serialposix.py

pymcuprog ping -d attiny412 -t uart -u /dev/ttyUSB0

cargo run --bin updi -- --path=/dev/ttyUSB0



  - Start bit is active low
  - 1 start bit
  - 2 stop bits
  - 'PARITY_EVEN'


  - 1 Kbps minimum (probably start with 115200)
- SYNC - 0x55
  - Read SIB which is 128 bits

- AtTiny512 Signautre
  - 0x1E 0x92 0x23


For ST or an STS, a ACK (0x40 is sent back)



device_id = self.protocol.memory_read(Avr8Protocol.AVR8_MEMTYPE_SRAM, 0x1100, 3)

*/


#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::{fs::File, time::Duration};

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::bundle::TaskResultBundle;
use executor::FileHandle;
use file::LocalPathBuf;
use macros::executor_main;
use peripherals::serial::{SerialPort, SerialOptions};

#[derive(Args)]
struct Args {
    path: LocalPathBuf,
}

struct UPDI {
    serial: SerialPort,
}

impl UPDI {
    async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.serial.write_all(data).await?;

        let mut echo = vec![0u8; data.len()];
        self.serial.read_exact(&mut echo).await?;

        if &echo[..] != data {
            return Err(err_msg("Did not echo back the same data"));
        }

        Ok(())
    }

}


#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    // Dobule break.
    {
        let mut options = SerialOptions::default();
        options.baud_rate = 300;
        options.num_parity_bits = 1;
        options.num_stop_bits = 1;
        options.odd_parity = false;

        let mut serial = SerialPort::open_with(&args.path, options)?;

        serial.write_all(&[
            0x00, // BREAK
        ]).await?;

        let mut buf = vec![0u8; 1];
        serial.read_exact(&mut buf).await?;

        executor::sleep(std::time::Duration::from_millis(100)).await?;

        serial.write_all(&[
            0x00, // BREAK
        ]).await?;

        let mut buf = vec![0u8; 1];
        serial.read_exact(&mut buf).await?;

        drop(serial);

        // executor::sleep(std::time::Duration::from_millis(100)).await?;
    }

    let mut options = SerialOptions::default();
    options.baud_rate = 115200;
    options.num_parity_bits = 1;
    options.num_stop_bits = 2;
    options.odd_parity = false;

    let mut serial = SerialPort::open_with(args.path, options)?;

    println!("Open...");


    let mut client = UPDI { serial };


    // STCS to UPDI_CS_CTRLB (0x03)
    client.send(&[
        0x55, 0xC3, 0x08,
    ]).await?;

    // STCS to UPDI_CS_CTRLA (0x02)
    client.send(&[
        0x55, 0xC2, 0x80
    ]).await?;

    // LDCS from UPDI_CS_STATUSA (0x00)
    client.send(&[
        0x55, 0x80
    ]).await?;

    println!("AA");

    let mut buf = vec![0u8; 1];
    client.serial.read_exact(&mut buf).await?;

    println!("First out: {:02x}", buf[0]);

    // Expect return of '0x20'

        // 

    client.send(&[
        // 0x55, 0xE6

        0x55, // SYNC

        // 0xE0 | 0x04 | 0x01

        0b11100101 // KEY: Send SIB
    ]).await?;

    // executor::sleep(std::time::Duration::from_millis(100)).await?;

    // serial.write_all(&[
    //     0x55, 0xE6

    //     // 0x55, // SYNC

    //     // 0xE0 | 0x04 | 0x01

    //     // 0b11100101 // KEY: Send SIB
    // ]).await?;

    let mut buf = vec![0u8; 64];

    println!("Sent");

    loop {
        let n = client.serial.read(&mut buf[..]).await?;

        // "tinyAVR P:0D:0-3"
        // NVM interface: 'P:0'

        // if n > 0 {
            println!("{}: {}", n, common::format::format_bytes(&buf[..n]));
        // }



    }

    Ok(())
}
