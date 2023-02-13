//! Driver for interfacing with Infinion Trust M ICs (e.g. SLS32AIA).
//!
//! Some notes on the protocol:
//!
//! - Communicates over I2C (typical speed is 400kHz, max 1MHz)
//! - At the outer most 'physical' layer, the device exposes reading/writing
//!   from registers (at 8-bit offsets).
//! - The DATA register at offset 0x80 is used to send/receive layered
//!   application requests/responses.
//! - Above the 'physical' layer, data is wrapped as follows:
//!     - 'Data layer' : Wraps data in checksumed frames
//!     - 'Network layer' : Multiplexes data into separate TX/RX 'channels'
//!     - `Transport layer' : Splits large payloads into multiple segmented
//!       frames
//!     - 'Application layer' : Has a standard format for request/responses.
//! - Integers are big endian

/*
I want to be able to:
- Store a ECC private key (secp256r1)
    - Call GetKeyPair to make a private key and return the public key
- Query its identifier
- Sign a digest using that key

max packet size: 0x110


Setup:
- Check I2C_STATE
- Set DATA_REG_LEN @ 0x81 to 0xFFFF (MAX_PACKET_SIZE = this - 5) (2 bytes)
- Read GUARD_TIME @ 0x85 (4 bytes)

Layering:
- Physical: Read/write from adress 0x80
- Data layer:
    - FCTR byte: See table 8.2.1
    - LEN (2 bytes) - Big endian
    - <data>
    - FCS (2 bytes checksum) - CCIT CRC-16 : x^16 + x^12 + x^5 + 1
        - Check for no more than 4093 bytes
        - Calculated over FCTR, LEN, data
- Network layer:
    - PCTR (1 byte) : Just set to zero unless using some fancy channels
- Transport laer:
    - Lower 3 bits of PCTR define if we are a beginning, middle, end, or single frame
- Application Layer
    - Request:
        - CMD (1 byte)
        - Param (1 byte)
        - Len (2 bytes)
        - DAta (len bytes)
    - Response:
        - Sta: 1 bytes : Response status code
        - Undef (1 byte) : Undefined
        - Len (2 bytes)
        - DAta (len bytes)

- ECC Key OIDs
    - ECC Key 1 (Infineon provisioned) : 0xE0F0
    - ECC Key 2 : 0xE0F1
*/

use std::time::{Duration, Instant};

use common::enum_def_with_unknown;
use common::errors::*;
use peripherals::i2c::I2CDevice;

const WRITE_TIMEOUT: Duration = Duration::from_millis(100);
const GUARD_TIME: Duration = Duration::from_micros(500);
const DEVICE_ADDRESS: u8 = 0x30;

const DATA_REG: u8 = 0x80;
const DATA_REG_LEN_REG: u8 = 0x81;
const I2C_STATE_REG: u8 = 0x82;
const GUARD_TIME_REG: u8 = 0x85;

const UNIQUE_APP_ID: &'static [u8; 16] = b"\xD2\x76\x00\x00\x04GenAuthAppl";

/// Interface for communicating to a connected Trust M device.
pub struct TrustM {
    bus: I2CDevice,
}

impl TrustM {
    pub fn open(mut bus: I2CDevice) -> Result<Self> {
        let mut inst = Self { bus };

        let i2c_state = inst.get_i2c_state()?;
        if i2c_state.raw != 0x08800000 {
            return Err(err_msg("Unexpected initial I2C state for device"));
        }

        // inst.write(&[DATA_REG_LEN_REG, 0xff, 0xff])?;
        // TODO: Read the guard time register.

        Ok(inst)
    }

    fn write(&mut self, data: &[u8]) -> Result<()> {
        let start = Instant::now();
        loop {
            let r = self.bus.write(DEVICE_ADDRESS, data);

            std::thread::sleep(GUARD_TIME);

            if r.is_ok() {
                return Ok(());
            }

            if start + WRITE_TIMEOUT > Instant::now() {
                continue;
            }

            return r;
        }
    }

    fn read(&mut self, data: &mut [u8]) -> Result<()> {
        self.bus.read(DEVICE_ADDRESS, data)?;
        std::thread::sleep(GUARD_TIME);
        Ok(())
    }

    fn get_i2c_state(&mut self) -> Result<I2CState> {
        self.write(&[I2C_STATE_REG])?;

        let mut buf = [0u8; 4];
        self.read(&mut buf)?;

        let raw = u32::from_be_bytes(buf);

        Ok(I2CState {
            busy: (raw >> 31) & 1 != 0,
            response_ready: (raw >> 30) & 1 != 0,
            soft_reset_supported: (raw >> 27) & 1 != 0,
            continue_read_supported: (raw >> 26) & 1 != 0,
            repeated_start_supported: (raw >> 25) & 1 != 0,
            clock_stretching_supported: (raw >> 24) & 1 != 0,
            presentation_layer_supported: (raw >> 23) & 1 != 0,
            read_length: (raw & 0xffff) as usize,
            raw,
        })
    }

    fn get_guard_time(&mut self) -> Result<[u8; 4]> {
        self.write(&[GUARD_TIME_REG])?;

        let mut buf = [0u8; 4];
        self.read(&mut buf)?;

        Ok(buf)
    }

    pub fn get_random(&mut self) -> Result<()> {
        let mut request = vec![];
        request.push(DATA_REG);
        // Request 8 bytes.
        Self::append_request(Command::GetRandom, 0, &[0, 8], &mut request);

        self.write(&request)?;

        let mut state;
        loop {
            state = self.get_i2c_state()?;
            if state.busy {
                println!("Busy!");
            } else {
                println!("{:?}", state);
                break;
            }
        }

        let mut response_req = [DATA_REG];
        self.write(&response_req)?;

        let mut response = vec![0u8; 64];
        self.read(&mut response[0..state.read_length])?;

        println!("{:?}", response);

        Ok(())
    }

    fn append_open_new_application_request(out: &mut Vec<u8>) {
        Self::append_request(
            Command::OpenApplication,
            0, // Initialize a clean app context
            UNIQUE_APP_ID,
            out,
        )
    }

    fn append_request(command: Command, param: u8, data: &[u8], out: &mut Vec<u8>) {
        Self::append_frame(
            0x03,
            |out| {
                out.push(0); // PCTR
                out.push(command as u8);
                out.push(param);
                out.extend_from_slice(&(data.len() as u16).to_be_bytes());
                out.extend_from_slice(data);
            },
            out,
        )
    }

    /// Appends a data layer frame to the given buffer.
    ///
    /// The format of a frame is:
    /// - FCTR : 1 byte
    /// - LEN : 2 bytes
    /// - DATA : LEN bytes
    /// - FCS : 2 bytes
    fn append_frame<F: Fn(&mut Vec<u8>)>(fctr: u8, data: F, out: &mut Vec<u8>) {
        let frame_start = out.len();

        out.push(fctr);

        let len_pos = out.len();
        out.push(0);
        out.push(0);

        let data_start = out.len();

        data(out);

        let data_len = out.len() - data_start;
        *array_mut_ref![out, len_pos, 2] = (data_len as u16).to_be_bytes();

        let fcs = Self::crc(&out[frame_start..]);
        out.extend_from_slice(&fcs);
    }

    /// Checksums frame data using a CRC-16 algorithm
    ///
    /// NOTE: This uses a reverse order of bits compared to the implementation
    /// in the crypto library.
    fn crc(data: &[u8]) -> [u8; 2] {
        let mut state: u16 = 0;
        for byte in data {
            let mut b = *byte;

            state ^= *byte as u16;

            for _ in 0..8 {
                if state & 1 != 0 {
                    state = (state >> 1) ^ 0x8408;
                } else {
                    state >>= 1;
                }
            }
        }

        state.to_be_bytes()
    }
}

#[derive(Debug, Clone)]
struct I2CState {
    busy: bool,
    response_ready: bool,
    soft_reset_supported: bool,
    continue_read_supported: bool,
    repeated_start_supported: bool,
    clock_stretching_supported: bool,
    presentation_layer_supported: bool,
    read_length: usize,

    raw: u32,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Command {
    GetDataObject = 0x01,
    SetDataObject = 0x02,
    SetObjectProtected = 0x03,
    GetRandom = 0x0C,
    EncryptSym = 0x14,
    DecryptSym = 0x15,
    EncryptAsym = 0x1E,
    DecryptAsym = 0x1F,
    CalcHash = 0x30,
    CalcSign = 0x31,
    VerifySign = 0x32,
    CalcSSec = 0x33,
    DeriveKey = 0x34,
    GetKeyPair = 0x38,
    GenSymKey = 0x39,
    OpenApplication = 0x70,
    CloseApplication = 0x71,
}

enum_def_with_unknown!(TrustMErrorCode u8 =>
    NoError = 0x00,
    InvalidOID = 0x01,
    InvalidParameterField = 0x03,
    InvalidLengthField = 0x04,
    InvalidParameterInDataField = 0x05,
    InternalProcessError = 0x06,
    AccessConditionsNotSatisfied = 0x07,
    DataObjectBoundaryExceeded = 0x08,
    MetadataTruncationError = 0x09,
    InvalidCommandField = 0x0A,
    CommandOutOfSequence = 0x0B,
    CommandNotAvailable = 0x0C,
    InsufficientMemory = 0x0D,
    CounterThresholdLimitExceeded = 0x0E,
    InvalidManifest = 0x0F,
    ActingOnInvalidMetadata = 0x11,
    UnsupportedExtensionOrId = 0x24,
    UnsupportedParams = 0x25,
    UnsupportedCertificate = 0x2A,
    SignatureVerificationFailure = 0x2C,
    IntegrityValidationFailure = 0x2D,
    DecryptionFailure = 0x2E,
    AuthorizationFailure = 0x2F
);

#[cfg(test)]
mod tests {
    use super::*;

    /*
    Example frames from the datasheet:

    OpenApplication: 80 03 00 15 00 70 00 00 10 D2 76 00 00 04 47 65 6E 41 75 74 68 41 70 70 6C 04 1A (27 bytes writen)

    80 03 00 15 00

    70
        00 - Start clean app
        00 10 - in len (16)
        D2 76 00 00 04 47 65 6E 41 75 74 68 41 70 70 6C
    */

    #[test]
    fn append_frame_test() {
        let data = hex!("00 70 00 00 10 D2 76 00 00 04 47 65 6E 41 75 74 68 41 70 70 6C");

        let expected_frame =
            hex!("03 00 15 00 70 00 00 10 D2 76 00 00 04 47 65 6E 41 75 74 68 41 70 70 6C 04 1A");

        let mut frame = vec![];
        TrustM::append_frame(0x03, |out| out.extend_from_slice(&data), &mut frame);

        assert_eq!(&frame, &expected_frame);

        frame.clear();
        TrustM::append_open_new_application_request(&mut frame);
        assert_eq!(&frame, &expected_frame);
    }
}
