#[macro_use]
extern crate common;
extern crate nix;

use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::{fs::File, time::Duration};

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::bundle::TaskResultBundle;
use executor::FileHandle;
use macros::executor_main;
use nix::{
    sys::termios::{
        cfgetispeed, cfgetospeed, cfsetispeed, cfsetospeed, tcgetattr, tcsetattr, BaudRate,
        ControlFlags, InputFlags, LocalFlags, OutputFlags,
    },
    unistd::isatty,
};

struct SerialPort {
    file: FileHandle,
}

#[async_trait]
impl Writeable for SerialPort {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.file.write(data).await
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

async fn serial_reader_thread(mut file: FileHandle) -> Result<()> {
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

async fn serial_writer_thread(mut file: FileHandle) -> Result<()> {
    let mut file = SerialPort { file };

    let mut stdin = file::Stdin::get();

    loop {
        let mut data = [0u8; 512];

        let mut n = stdin.read(&mut data).await?;
        if n == 0 {
            println!("EOI");
            break;
        }

        println!("Write>");
        // file.write(&mut data[0..n]).await?;
        file.write_all(&mut data[0..n]).await?;
        println!("DoneWrite");
    }

    Ok(())
}

/*
TODO: Can't detect serial port disconnect.
*/

#[executor_main]
async fn main() -> Result<()> {
    let mut file = file::LocalFile::open_with_options(
        "/dev/ttyACM0",
        file::LocalFileOpenOptions::new().read(true).write(true),
    )?;

    // TODO: This seems to trigger a reset of the device?

    // ioctl(TCGETS, *mut termios)
    let mut termios = tcgetattr(unsafe { file.as_raw_fd() })?;

    println!("{:?}", termios);

    cfsetispeed(&mut termios, BaudRate::B115200)?;
    cfsetospeed(&mut termios, BaudRate::B115200)?;

    termios.input_flags = InputFlags::empty();
    termios.local_flags = LocalFlags::empty();
    termios.output_flags = OutputFlags::empty();
    // TODO: Figure out best output_flags.

    tcsetattr(
        unsafe { file.as_raw_fd() },
        nix::sys::termios::SetArg::TCSAFLUSH,
        &termios,
    )?;

    // TODO: Mark as not seekable.
    let mut reader = unsafe { file.into_raw_handle() };
    unsafe { reader.set_not_seeekable() };
    let writer = reader.clone();

    let mut bundle = TaskResultBundle::new();
    bundle.add("SerialRead", serial_reader_thread(reader));
    bundle.add("SerialWrite", serial_writer_thread(writer));

    bundle.join().await?;

    Ok(())
}
