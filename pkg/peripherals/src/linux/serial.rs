use core::convert::TryFrom;

use alloc::boxed::Box;

use common::errors::*;
use common::io::{Readable, SharedWriteable, Writeable};
use executor::linux::FileHandle;
use file::LocalPath;

use sys::bindings::serial_struct;
use sys::bindings::termios2;

// tcgetattr(fd, argp)
ior!(tcgets2, b'T', 0x2A, termios2);

// tcsetattr(fd, TCSANOW, argp)
iow!(tcsets2, b'T', 0x2B, termios2);

// tcsetattr(fd, TCSADRAIN, argp)
iow!(tcsetsw2, b'T', 0x2C, termios2);

// tcsetattr(fd, TCSAFLUSH, argp)
iow!(tcsetsf2, b'T', 0x2D, termios2);

ior!(tiocgserial, b'T', 0x1E, serial_struct);
iow!(tiocsserial, b'T', 0x1F, serial_struct);

// #define TIOCGSERIAL     0x541E
// #define TIOCSSERIAL     0x541F
// serial_struct

pub struct SerialOptions {
    pub baud_rate: usize,
    pub num_stop_bits: usize,
    pub num_parity_bits: usize,
    pub num_data_bits: usize,
    pub odd_parity: bool,
}

impl Default for SerialOptions {
    fn default() -> Self {
        Self {
            baud_rate: 115200,
            num_stop_bits: 1,
            num_parity_bits: 0,
            num_data_bits: 8,
            odd_parity: false
        }
    }
}

pub struct SerialPort {
    file: FileHandle,
}

impl SerialPort {
    pub fn open<P: AsRef<LocalPath>>(path: P, baud_rate: usize) -> Result<Self> {
        let mut options = SerialOptions::default();
        options.baud_rate = baud_rate;
        Self::open_with(path, options)
    }

    pub fn open_with<P: AsRef<LocalPath>>(path: P, options: SerialOptions) -> Result<Self> {
        let path = path.as_ref();

        if !path.as_str().starts_with("/dev/tty") {
            return Err(err_msg("Must open a /dev/tty* file on linux."));
        }

        // TODO: This seems to trigger a reset of Arduino based devices?
        let file = file::LocalFile::open_with_options(
            path,
            file::LocalFileOpenOptions::new().read(true).write(true)
        )?;

        // Setting up 8N1 UART (no flow control) at the requested baud rate.
        {
            let mut t = termios2::default();
            unsafe { tcgets2(file.as_raw_fd(), &mut t) }?;

            t.c_iflag = 0;
            t.c_oflag = 0;
            t.c_lflag = 0;

            t.c_cflag = sys::bindings::CREAD // Enable the receiver
                | sys::bindings::CLOCAL // This prevents future re-opens of the port to block.
                | sys::bindings::BOTHER
                | (sys::bindings::BOTHER << sys::bindings::IBSHIFT);

            t.c_ispeed = options.baud_rate as u32;
            t.c_ospeed = options.baud_rate as u32;

            t.c_cflag |= match options.num_data_bits {
                5 => sys::bindings::CS5,
                6 => sys::bindings::CS6,
                7 => sys::bindings::CS7,
                8 => sys::bindings::CS8,
                _ => return Err(format_err!("Unsupported number of data bits: {}", options.num_data_bits))
            };

            match options.num_stop_bits {
                1 => {},
                2 => {
                    t.c_cflag |= sys::bindings::CSTOPB;
                }
                _ => return Err(err_msg("Unsupported number of stop bits"))
            };

            match options.num_parity_bits {
                0 => {}
                1 => {
                    t.c_cflag |= sys::bindings::PARENB;
                }
                _ => return Err(err_msg("Unsupported number of parity bits"))
            }

            if options.odd_parity {
                t.c_cflag |= sys::bindings::PARODD;
            }

            unsafe { tcsetsf2(file.as_raw_fd(), &t) }?;
        }

        // For FTDI chips, the chip by default waits 16ms before responding to a USB
        // packet if the buffer doesn't have enough data (62 bytes) to send back.
        // Setting this flag changes this timeout to 1ms.
        //
        // Note that a 250k baud serial connection will generate at most 25 bytes per
        // millisecond so there may be ineffeciencies at low speeds, though at speeds
        // like 620k baud, the buffer should always fill up in high throughput
        // situations.
        //
        // This will speed up known to be small responses like "ok\n" on grbl style
        // devices. A less generic solution is to use the event character feature to
        // have the chip return data as soon as a character like "\n" is seen.
        //
        // See https://ftdichip.com/wp-content/uploads/2020/08/AN232B-04_DataLatencyFlow.pdf
        /*
        {
            let latency_timer = format!(
                "/sys/bus/usb-serial/devices/{}/latency_timer",
                path.file_name().unwrap()
            );
            if file::exists_sync(LocalPath::new(&latency_timer))? {
                std::fs::write(latency_timer, "4")?;
            }
        }
        */
        /*
        // TODO: Figure out why this doesn't work.
        {
            let mut s = serial_struct::default();
            unsafe { tiocgserial(file.as_raw_fd(), &mut s) }?;

            println!("{:?}", s);

            // Equivalent to latency_timer = 1
            s.flags |= sys::bindings::ASYNC_LOW_LATENCY as i32;

            unsafe { tiocsserial(file.as_raw_fd(), &s) }?;
        }
        */

        let mut handle = unsafe { file.into_raw_handle() };
        unsafe { handle.set_not_seekable() };

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
