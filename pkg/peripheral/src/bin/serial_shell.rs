extern crate common;
extern crate nix;

use std::fs::File;
use std::io::Read;
use std::os::unix::io::AsRawFd;

use common::errors::*;
use nix::{
    sys::termios::{
        cfgetispeed, cfgetospeed, cfsetispeed, cfsetospeed, tcgetattr, tcsetattr, BaudRate,
        ControlFlags, InputFlags, LocalFlags, OutputFlags,
    },
    unistd::isatty,
};

fn main() -> Result<()> {
    let mut file = File::open("/dev/ttyUSB0")?;

    // ioctl(TCGETS, *mut termios)
    let mut termios = tcgetattr(file.as_raw_fd())?;
    cfsetispeed(&mut termios, BaudRate::B115200)?;
    cfsetospeed(&mut termios, BaudRate::B115200)?;

    termios.input_flags = InputFlags::empty();
    termios.local_flags = LocalFlags::empty();
    // TODO: Figure out best output_flags.

    tcsetattr(
        file.as_raw_fd(),
        nix::sys::termios::SetArg::TCSAFLUSH,
        &termios,
    )?;

    loop {
        let mut buf = vec![];
        buf.resize(256, 0);

        let nread = file.read(&mut buf)?;
        if nread == 0 {
            println!("<Closed>");
            break;
        }

        println!("{:?}", common::bytes::Bytes::from(&buf[0..nread]));
    }

    Ok(())
}
