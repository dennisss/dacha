use core::convert::TryFrom;

use alloc::boxed::Box;

use common::errors::*;
use common::io::{Readable, SharedWriteable, Writeable};
use executor::linux::FileHandle;
use file::LocalPath;
use nix::{
    sys::termios::{
        cfgetispeed, cfgetospeed, cfsetispeed, cfsetospeed, tcgetattr, tcsetattr, BaudRate,
        ControlFlags, InputFlags, LocalFlags, OutputFlags,
    },
    unistd::isatty,
};

pub struct SerialPort {
    file: FileHandle,
}

impl SerialPort {
    pub fn open<P: AsRef<LocalPath>>(path: P, baud_rate: usize) -> Result<Self> {
        let path = path.as_ref();

        if !path.as_str().starts_with("/dev/tty") {
            return Err(err_msg("Must open a /dev/tty* file on linux."));
        }

        let baud = {
            use BaudRate::*;

            match baud_rate {
                110 => B110,
                134 => B134,
                150 => B150,
                200 => B200,
                300 => B300,
                600 => B600,
                1200 => B1200,
                1800 => B1800,
                2400 => B2400,
                4800 => B4800,
                9600 => B9600,
                19200 => B19200,
                38400 => B38400,
                57600 => B57600,
                115200 => B115200,
                230400 => B230400,
                460800 => B460800,
                500000 => B500000,
                576000 => B576000,
                921600 => B921600,
                1000000 => B1000000,
                1152000 => B1152000,
                1500000 => B1500000,
                2000000 => B2000000,
                2500000 => B2500000,
                3000000 => B3000000,
                3500000 => B3500000,
                4000000 => B4000000,
                _ => return Err(err_msg("Unknown baud rate.")),
            }
        };

        // TODO: This seems to trigger a reset of Arduino based devices?
        let file = file::LocalFile::open_with_options(
            path,
            file::LocalFileOpenOptions::new().read(true).write(true),
        )?;

        // ioctl(TCGETS, *mut termios)
        let mut termios = tcgetattr(unsafe { file.as_raw_fd() })?;

        cfsetispeed(&mut termios, baud)?;
        cfsetospeed(&mut termios, baud)?;

        termios.input_flags = InputFlags::empty();
        termios.local_flags = LocalFlags::empty();
        termios.output_flags = OutputFlags::empty();

        tcsetattr(
            unsafe { file.as_raw_fd() },
            nix::sys::termios::SetArg::TCSAFLUSH,
            &termios,
        )?;

        let mut handle = unsafe { file.into_raw_handle() };
        unsafe { handle.set_not_seeekable() };

        Ok(Self { file: handle })
    }

    pub fn split(mut self) -> (Box<dyn Readable + Sync>, Box<dyn SharedWriteable>) {
        let reader = Box::new(Self {
            file: self.file.clone(),
        });

        (reader, Box::new(self))
    }
}

#[async_trait]
impl Readable for SerialPort {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.file.read(output).await
    }
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
