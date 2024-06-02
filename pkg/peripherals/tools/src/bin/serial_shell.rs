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
use peripherals::serial::SerialPort;

#[derive(Args)]
struct Args {
    path: LocalPathBuf,
    baud: usize,
}

async fn serial_reader_thread(mut file: Box<dyn Readable>) -> Result<()> {
    loop {
        let mut buf = vec![];
        buf.resize(256, 0);

        let nread = file.read(&mut buf).await?;
        if nread == 0 {
            println!("<Reader Closed>");
            break;
        }

        println!("{:?}", common::bytes::Bytes::from(&buf[0..nread]));
    }

    Ok(())
}

async fn serial_writer_thread(mut file: Box<dyn Writeable>) -> Result<()> {
    let mut stdin = file::Stdin::get();

    loop {
        let mut data = [0u8; 512];

        let mut n = stdin.read(&mut data).await?;
        if n == 0 {
            println!("EOI");
            break;
        }

        // file.write(&mut data[0..n]).await?;
        file.write_all(&mut data[0..n]).await?;
    }

    Ok(())
}

/*
TODO: Can't detect serial port disconnect.
*/

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let file = SerialPort::open(args.path, args.baud)?;

    let (reader, writer) = file.split();

    let mut bundle = TaskResultBundle::new();
    bundle.add("SerialRead", serial_reader_thread(reader));
    bundle.add("SerialWrite", serial_writer_thread(writer));

    bundle.join().await?;

    Ok(())
}
