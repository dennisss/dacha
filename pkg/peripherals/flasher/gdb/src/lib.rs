#[macro_use]
extern crate macros;

use std::{
    io::{Read, Write},
    time::Duration,
};

use base_error::*;
use base_util::format::format_bytes;
use common::{
    fixed::vec::FixedVec,
    io::{Readable, Writeable},
};
use peripherals::serial::SerialPort;

const PACKET_START_CHAR: u8 = b'$';
const PACKET_NOTIFICATION_START_CHAR: u8 = b'%';
const PACKET_END_CHAR: u8 = b'#';
const PACKET_ESCAPE_CHAR: u8 = b'}';

const PACKET_ACK_CHAR: u8 = b'+';
const PACKET_NACK_CHAR: u8 = b'-';

const RECEIVE_BUFFER_SIZE: usize = 4096;

/// Number of bytes per send/received block of data.
const TRANSFER_BLOCK_SIZE: usize = 256;

const OK_RESPONSE: &'static [u8] = b"OK";

/// Interface for talking to a remote target machine implementing the GDB
/// protocol via serial. The currently running machine is treated as the 'host'.
///
/// The GDB protocol is documented here:
/// - https://sourceware.org/gdb/current/onlinedocs/gdb.html/Remote-Protocol.html#Remote-Protocol
///
/// Information about what is supported by Black Magic Probes can be found in
/// the code here:
/// - https://github.com/blackmagic-debug/blackmagic/blob/f4e79b65d4dc8bee6df74348150da36d8e112794/src/gdb_packet.c
pub struct RemoteTarget {
    serial: SerialPort,

    send_buffer: Vec<u8>,

    received_buffer: Vec<u8>,
    received_buffer_pos: usize,
}

impl RemoteTarget {
    /// Finds a USB connected 'black magic compatible probe' and connects to it.
    pub async fn connect() -> Result<Self> {
        let usb_context = usb::Context::create()?;
        let devices = usb_context.enumerate_devices().await?;

        let mut found_device = None;
        for dev in devices {
            let desc = dev.device_descriptor()?;
            if desc.idVendor == 0x1d50 && desc.idProduct == 0x6018 {
                if found_device.is_some() {
                    return Err(err_msg("Multiple probes found"));
                }

                found_device = Some(dev);
            }
        }

        let device = found_device.ok_or_else(|| err_msg("No probe device found"))?;

        let driver = device
            .driver_devices()
            .await?
            .into_iter()
            .find(|driver| driver.typ == usb::DriverDeviceType::TTY && driver.interface_num == 0)
            .ok_or_else(|| err_msg("Unable to find the probe serial interface"))?;

        let serial = peripherals::serial::SerialPort::open(driver.path, 9600)?;

        Ok(Self {
            serial,
            send_buffer: vec![],
            received_buffer: vec![],
            received_buffer_pos: 0,
        })
    }

    pub async fn query_supported(&mut self) -> Result<()> {
        self.send_request_packet(b"qSupported").await?;

        let res = self.receive_packet(true).await?;

        println!("Supported: {}", core::str::from_utf8(&res[..])?);

        Ok(())
    }

    /// For an NRF52840 the map will look like:
    ///
    /// <memory-map><memory type="ram" start="0x20000000"
    /// length="0x40000"/><memory type="flash" start="0x10001000"
    /// length="0x1000"><property
    /// name="blocksize">0x1000</property></memory><memory type="flash"
    /// start="0x00000000" length="0x100000"><property
    /// name="blocksize">0x1000</property></memory></memory-map>
    pub async fn read_memory_map(&mut self) -> Result<()> {
        let mut data = vec![];

        loop {
            self.send_request_packet(
                format!(
                    "qXfer:memory-map:read::{:x},{:x}",
                    data.len(),
                    TRANSFER_BLOCK_SIZE
                )
                .as_bytes(),
            )
            .await?;

            let packet_data = self.receive_packet(true).await?;

            if &packet_data == b"l" {
                // addr == data.len()
                break;
            }

            let inner_data = packet_data.strip_prefix(b"m").ok_or_else(|| {
                format_err!(
                    "Unknown format for map data: {}",
                    format_bytes(&packet_data)
                )
            })?;
            data.extend_from_slice(inner_data);

            if inner_data.len() < TRANSFER_BLOCK_SIZE {
                break;
            }
        }

        println!("Memory Map: {}", core::str::from_utf8(&data)?);

        Ok(())
    }

    pub async fn send_rcmd(&mut self, command: &[u8]) -> Result<Vec<u8>> {
        self.send_request_packet(format!("qRcmd,{}", base_radix::hex_encode(command)).as_bytes())
            .await?;

        loop {
            let packet_data = self.receive_packet(false).await?;

            if &packet_data[..] == OK_RESPONSE {
                return Ok(vec![]);
            }

            if packet_data[0] == b'O' {
                let text = base_radix::hex_decode(core::str::from_utf8(&packet_data[1..])?)?;
                println!("[gdb console] {}", format_bytes(&text));
                continue;
            }

            return Ok(base_radix::hex_decode(core::str::from_utf8(&packet_data)?)?);
        }
    }

    pub async fn attach(&mut self, pid: usize) -> Result<()> {
        self.send_request_packet(format!("vAttach;{:x}", pid).as_bytes())
            .await?;

        let res = self.receive_packet(true).await?;

        // TODO: Parse it
        // Response is either:
        // - Any stop packet
        // - OK

        Ok(())
    }

    pub async fn detach(&mut self) -> Result<()> {
        self.send_request_packet(b"D").await?;

        let res = self.receive_packet(true).await?;
        if res != OK_RESPONSE {
            return Err(err_msg("Bad response to detach"));
        }

        Ok(())
    }

    pub async fn kill(&mut self, pid: usize) -> Result<()> {
        self.send_request_packet(format!("vKill;{:x}", pid).as_bytes())
            .await?;

        let res = self.receive_packet(true).await?;
        if res != OK_RESPONSE {
            return Err(err_msg("Bad response to kill"));
        }

        Ok(())
    }

    pub async fn read_memory(&mut self, mut addr: usize, out: &mut [u8]) -> Result<()> {
        let mut i = 0;
        while i < out.len() {
            let n = core::cmp::min(TRANSFER_BLOCK_SIZE, out.len() - i);
            self.read_memory_block(addr + i, &mut out[i..(i + n)])
                .await?;
            i += n;
        }

        Ok(())
    }

    async fn read_memory_block(&mut self, addr: usize, out: &mut [u8]) -> Result<()> {
        self.send_request_packet(format!("m{:x},{:x}", addr, out.len()).as_bytes())
            .await?;

        let res = self.receive_packet(true).await?;
        let data = base_radix::hex_decode(core::str::from_utf8(&res[..])?)?;
        if data.len() != out.len() {
            return Err(err_msg("Wrong number of bytes read"));
        }

        out.copy_from_slice(&data);

        Ok(())
    }

    /// NOTE: This will automatically chunk the data to fit within reasonable
    /// packet size limits.
    ///
    /// TODO: Differentiate this with the 'M' command that also writes memory.
    pub async fn write_memory(&mut self, mut addr: usize, data: &[u8]) -> Result<()> {
        let mut i = 0;
        while i < data.len() {
            let n = core::cmp::min(TRANSFER_BLOCK_SIZE, data.len() - i);
            self.write_memory_block(addr + i, &data[i..(i + n)]).await?;
            i += n;
        }

        Ok(())
    }

    async fn write_memory_block(&mut self, addr: usize, data: &[u8]) -> Result<()> {
        let mut packet_data = vec![];
        packet_data.extend_from_slice(format!("X{:x},{:x}:", addr, data.len()).as_bytes());
        packet_data.extend_from_slice(data);
        self.send_request_packet(&packet_data).await?;

        let res = self.receive_packet(true).await?;
        if res != OK_RESPONSE {
            return Err(err_msg("Bad response to kill"));
        }

        Ok(())
    }

    pub async fn flash_erase(&mut self, addr: usize, length: usize) -> Result<()> {
        self.send_request_packet(format!("vFlashErase:{:x},{:x}", addr, length).as_bytes())
            .await?;

        let res = self.receive_packet(true).await?;
        if res != OK_RESPONSE {
            return Err(err_msg("Bad response to kill"));
        }

        Ok(())
    }

    /// NOTE: This will automatically chunk the data to fit within reasonable
    /// packet size limits.
    pub async fn flash_write(&mut self, mut addr: usize, data: &[u8]) -> Result<()> {
        let mut i = 0;
        while i < data.len() {
            let n = core::cmp::min(TRANSFER_BLOCK_SIZE, data.len() - i);
            self.flash_write_block(addr + i, &data[i..(i + n)]).await?;
            i += n;
        }

        Ok(())
    }

    async fn flash_write_block(&mut self, addr: usize, data: &[u8]) -> Result<()> {
        let mut packet_data = vec![];
        packet_data.extend_from_slice(format!("vFlashWrite:{:x}:", addr).as_bytes());
        packet_data.extend_from_slice(data);
        self.send_request_packet(&packet_data).await?;

        let res = self.receive_packet(true).await?;
        if res != OK_RESPONSE {
            return Err(err_msg("Bad response to kill"));
        }

        Ok(())
    }

    pub async fn flash_done(&mut self) -> Result<()> {
        self.send_request_packet(b"vFlashDone").await?;

        let res = self.receive_packet(true).await?;
        if res != OK_RESPONSE {
            return Err(err_msg("Bad response to kill"));
        }

        Ok(())
    }

    /// Sends a request packet and waits for it to be acknowledged by the
    /// target.
    ///
    /// DOES NOT parse and acknowledge all the packets returned as a response to
    /// the request packet.
    async fn send_request_packet(&mut self, data: &[u8]) -> Result<()> {
        if !self.received_buffer.is_empty() {
            return Err(err_msg(
                "Unprocessed responses before sending the next request.",
            ));
        }

        self.send_buffer.clear();
        Self::create_request_packet(data, &mut self.send_buffer);

        self.serial.write_all(&self.send_buffer).await?;
        self.send_buffer.clear();

        self.received_buffer.resize(RECEIVE_BUFFER_SIZE, 0);
        let n = self.serial.read(&mut self.received_buffer[..]).await?;
        if n == 0 {
            return Err(err_msg("Received no response to request"));
        }
        self.received_buffer.truncate(n);

        // println!("RX: {}", format_bytes(&self.received_buffer[..]));

        if self.received_buffer[0] == PACKET_NACK_CHAR {
            return Err(err_msg("Received NAK"));
        }

        if self.received_buffer[0] != PACKET_ACK_CHAR {
            return Err(err_msg("Unknown format for ACK to request"));
        }

        self.received_buffer_pos = 1;

        Ok(())
    }

    /// NOTE: This doesn't escape '*' so shouldn't be used for creating
    /// responses.
    fn create_request_packet(data: &[u8], out: &mut Vec<u8>) {
        out.push(PACKET_START_CHAR);

        let mut sum: u8 = 0;
        for mut b in data.iter().cloned() {
            if b == PACKET_START_CHAR || b == PACKET_ESCAPE_CHAR || b == PACKET_END_CHAR {
                // Escape

                out.push(PACKET_ESCAPE_CHAR);
                sum = sum.wrapping_add(PACKET_ESCAPE_CHAR);

                b ^= 0x20;
            }

            sum = sum.wrapping_add(b);
            out.push(b);
        }

        out.push(PACKET_END_CHAR);
        out.extend_from_slice(format!("{:02X}", sum).as_bytes());
    }

    async fn receive_packet(&mut self, allow_error_response: bool) -> Result<Vec<u8>> {
        loop {
            let (data, is_notification) = self.receive_packet_raw().await?;
            if is_notification {
                println!("[gdb notification] {}", format_bytes(&data));
                continue;
            }

            // Ack the packet.
            self.send_buffer.clear();
            self.send_buffer.push(PACKET_ACK_CHAR);
            self.serial.write_all(&self.send_buffer[..]).await?;

            // Empty response packets mean we sent a command that wasn't supported.
            if data.is_empty() {
                return Err(err_msg(
                    "Empty response packet received (unsupported command)",
                ));
            }

            if allow_error_response {
                if let Some(message) = data.strip_prefix(b"E.") {
                    return Err(format_err!(
                        "Received error message: {}",
                        format_bytes(message)
                    ));
                }

                if let Some(code) = data.strip_prefix(b"E") {
                    if code.len() != 2 {
                        return Err(err_msg("Recieved error code that isn't two bytes."));
                    }

                    let code = u8::from_str_radix(core::str::from_utf8(code)?, 16)?;

                    return Err(format_err!("Received error code {}", code));
                }
            }

            return Ok(data);
        }
    }

    async fn receive_packet_raw(&mut self) -> Result<(Vec<u8>, bool)> {
        let mut data = vec![];

        let mut parser = ResponsePacketParser::new();
        loop {
            if self.received_buffer_pos == self.received_buffer.len() {
                self.received_buffer.resize(RECEIVE_BUFFER_SIZE, 0);
                let n = self.serial.read(&mut self.received_buffer[..]).await?;
                self.received_buffer.truncate(n);
                self.received_buffer_pos = 0;

                if n == 0 {
                    return Err(err_msg("Hit end of serial port without a complete packet"));
                }

                // println!("RX: {}", format_bytes(&self.received_buffer[..]));
            }

            let (done, rest) =
                parser.parse(&self.received_buffer[self.received_buffer_pos..], &mut data)?;
            self.received_buffer_pos = self.received_buffer.len() - rest.len();
            if self.received_buffer_pos == self.received_buffer.len() {
                self.received_buffer.clear();
                self.received_buffer_pos = 0;
            }

            if done {
                break;
            }
        }

        Ok((data, parser.is_notification()))
    }
}

struct ResponsePacketParser {
    state: ResponsePacketParserState,

    sum: u8,

    escaped: bool,

    is_notification: bool,

    received_sum: FixedVec<u8, 2>,
}

enum ResponsePacketParserState {
    /// Reading the first 'start' byte.
    Start,

    /// Reading packet data.
    Data,

    /// Reading the 2 checksum bytes.
    Checksum,

    Done,
}

impl ResponsePacketParser {
    fn new() -> Self {
        Self {
            state: ResponsePacketParserState::Start,
            sum: 0,
            escaped: false,
            received_sum: FixedVec::new(),
            is_notification: false,
        }
    }

    /// Parses part or all of one packet.
    ///
    /// Args:
    /// - data: Data received from the remote machine.
    /// - out: Place to store any unescaped packet data (will be partially
    ///   filled if only part of the packet was decoded).
    ///
    /// Returns: whether or not we finished decoding a complete packet and any
    /// remaining data left over from 'data' that may be part of the next
    /// packet. This function will always either parse an entire packet or
    /// consume all of the input data.
    fn parse<'a>(&mut self, mut data: &'a [u8], out: &mut Vec<u8>) -> Result<(bool, &'a [u8])> {
        while !data.is_empty() {
            match self.state {
                ResponsePacketParserState::Start => {
                    match data[0] {
                        PACKET_START_CHAR => {
                            // Normal
                        }
                        PACKET_NOTIFICATION_START_CHAR => {
                            self.is_notification = true;
                        }
                        _ => {
                            return Err(format_err!(
                                "Unsupported packet start character: 0x{:02x}",
                                data[0]
                            ));
                        }
                    }

                    data = &data[1..];
                    self.state = ResponsePacketParserState::Data;
                }
                ResponsePacketParserState::Data => {
                    let mut b = data[0];
                    data = &data[1..];

                    if b == PACKET_END_CHAR {
                        self.state = ResponsePacketParserState::Checksum;
                        continue;
                    }

                    self.sum = self.sum.wrapping_add(b);

                    if b == PACKET_ESCAPE_CHAR {
                        self.escaped = true;
                        continue;
                    }

                    if self.escaped {
                        b ^= 0x20;
                        self.escaped = false;
                    }

                    out.push(b);
                }
                ResponsePacketParserState::Checksum => {
                    self.received_sum.push(data[0]);
                    data = &data[1..];

                    if self.received_sum.len() < 2 {
                        continue;
                    }

                    let expected_sum =
                        u8::from_str_radix(core::str::from_utf8(self.received_sum.as_ref())?, 16)?;

                    if expected_sum != self.sum {
                        return Err(err_msg("Wrong checksum"));
                    }

                    self.state = ResponsePacketParserState::Done;

                    return Ok((true, data));
                }

                ResponsePacketParserState::Done => {
                    *self = Self::new();
                }
            }
        }

        Ok((false, data))
    }

    fn is_notification(&self) -> bool {
        self.is_notification
    }
}
