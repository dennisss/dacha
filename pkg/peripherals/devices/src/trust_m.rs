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
    - Separate register at 0x82 is used for checking the status of operations.
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
use parsing::binary::be_u16;
use parsing::binary::be_u8;
use parsing::take_exact;
use peripherals::i2c::I2CHostController;
use peripherals::i2c::I2CHostDevice;

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
    device: I2CHostDevice,

    /// Index (mod 4) of the next frame to be sent.
    next_frame_counter: u8,
}

impl TrustM {
    pub async fn open(mut bus: &I2CHostController) -> Result<Self> {
        let mut inst = Self {
            device: bus.device(DEVICE_ADDRESS),
            next_frame_counter: 0,
        };

        inst.soft_reset().await?;

        // Most commands require an application to be open, so
        inst.open_new_application().await?;

        Ok(inst)
    }

    async fn soft_reset(&mut self) -> Result<()> {
        // Soft reset by writing to register 0x88 (2 byte register)
        self.write(&[0x88, 0xFF, 0xFF]).await?;

        let i2c_state = self.get_i2c_state().await?;

        // busy: false,
        // response_ready: false,
        // soft_reset_supported: true,
        // read_length: 0,
        if i2c_state.raw != 0x8000000 {
            return Err(format_err!(
                "Unexpected initial state for TPM: {:?}",
                i2c_state
            ));
        }

        self.next_frame_counter = 0;
        Ok(())
    }

    // fn next_data_fctr(&mut self) -> FCTR {
    //     let last_frame_counter =
    // }

    async fn open_new_application(&mut self) -> Result<()> {
        let data = self
            .call(
                Command::OpenApplication,
                0, // Initialize a clean app context
                UNIQUE_APP_ID,
            )
            .await?;

        if !data.is_empty() {
            return Err(err_msg(
                "Expected no data to be returned from OpenApplication",
            ));
        }

        Ok(())
    }

    pub async fn read_coprocessor_uid(&mut self) -> Result<Vec<u8>> {
        self.call(
            Command::GetDataObject,
            0x00,
            &[0xE0, 0xC2, 0x00, 0x00, 0x00, 0x64],
        )
        .await
    }

    // TODO: Convert these into goldens to be verified in unit tests.
    /*
    pub async fn read_coprocessor_uid(&mut self) -> Result<()> {
        // Step 1: Send OpenApplication command.
        {
            let mut request = vec![];
            request.push(DATA_REG);
            Self::append_open_new_application_request(&mut request);
            self.write(&request).await?;
        }

        // Step 2: Poll state until response data is ready
        // Expect the state to be rougly 'C8 80 00 05'
        // - busy: true
        // - response_ready: true,
        // - read_length: 5,
        let mut state;
        loop {
            state = self.get_i2c_state().await?;
            if state.read_length == 0 {
                println!("Busy!");
            } else {
                // println!("Raw: {:0x?}", state.raw);
                // println!("{:?}", state);
                break;
            }
        }

        // Step 3: Read the Ack for the command.
        {
            if state.read_length != 5 {
                return Err(err_msg("Incorrect length for ACK response"));
            }

            self.write(&[DATA_REG]).await?;

            let mut ack = vec![0u8; 5];
            self.read(&mut ack).await?;

            // Expect '80 00 00 0C EC'
            // - 0x80: FCTR : Control Frame , ACK for frame 0
            // - 0x00, 0x00 : Length
            // - [Checksum]
            // println!("Step 3: {:0x?}", ack);
            if ack != &[0x80, 0x00, 0x00, 0x0C, 0xEC] {
                return Err(format_err!("Bad ACK received: {:02x?}", ack));
            }
        }

        // Step 4
        // Waiting for state to be '48 80 00 0A'
        // - busy: false
        // - response_ready: true
        // - read_length: 10
        loop {
            state = self.get_i2c_state().await?;
            if state.read_length == 0 {
                println!("Busy!");
            } else {
                // println!("Raw: {:0x?}", state.raw);
                // println!("{:?}", state);
                break;
            }
        }

        // Step 5: Read response for the OpenApplication command
        // Expect '00 00 05 00 00 00 00 00 14 87'
        // - 00 : FCTR : Data Frame 0
        // - 00 05 LEN : LEN
        // - ...
        // - 14 87 : CHECKSUM
        {
            if state.read_length != 10 {
                return Err(err_msg("Wrong length for OpenApplication response"));
            }

            self.write(&[DATA_REG]).await?;

            let mut response = vec![0u8; 10];
            self.read(&mut response).await?;
            // println!("Step 5: {:0x?}", response);

            if response != &[0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x87] {
                return Err(format_err!(
                    "Bad OpenApplication response received: {:02x?}",
                    response
                ));
            }
        }

        // Step 6. Send ACK
        // - 80 : FCTR: Control frame 0, ACK for frame 0
        // - 00 00 : LEN
        // - 0C EC : CHECKSUM
        self.write(&[DATA_REG, 0x80, 0x00, 0x00, 0x0C, 0xEC])
            .await?;

        // Executing the 'Read Coprocessor UID' example in the datasheet.

        // Step 1: Send GetDataObject
        // - 04 : FCTR : Data Frame 1, ACK for frame 0
        //
        // In the request:
        // - 00 : PCTR
        // - 01 : CMD for GetDataObject
        // - 00 : Param: 'Read Data'
        // - 00 06 : InLen
        // - E0 C2 : OID for Coprocessor ID data
        // - 00 00 : Offset in the data
        // - 00 64 : Number of bytes to read
        {
            let mut request = vec![DATA_REG];
            Self::append_frame(
                0x04,
                |out| {
                    out.extend_from_slice(&[
                        0x00, 0x01, 0x00, 0x00, 0x06, 0xE0, 0xC2, 0x00, 0x00, 0x00, 0x64,
                    ]);
                },
                &mut request,
            );

            assert_eq!(
                &request,
                &[
                    DATA_REG, 0x04, 0x00, 0x0B, 0x00, 0x01, 0x00, 0x00, 0x06, 0xE0, 0xC2, 0x00,
                    0x00, 0x00, 0x64, 0xF0, 0x9F,
                ]
            );

            self.write(&request).await?;
        }

        // Step 2: Read I2C_STATE
        let mut state;
        loop {
            state = self.get_i2c_state().await?;
            if state.read_length == 0 {
                println!("Busy!");
            } else {
                println!("=== Raw: {:0x?}", state.raw);
                println!("{:?}", state);
                break;
            }
        }

        // Step 2.5
        {
            if state.read_length != 5 {
                return Err(err_msg("Incorrect length for ACK response"));
            }

            self.write(&[DATA_REG]).await?;

            let mut ack = vec![0u8; 5];
            self.read(&mut ack).await?;

            println!("ACK: {:02x?}", ack);

            // // Expect '80 00 00 0C EC'
            // // - 0x80: FCTR : Control Frame , ACK for frame 0
            // // - 0x00, 0x00 : Length
            // // - [Checksum]
            // // println!("Step 3: {:0x?}", ack);
            // if ack != &[0x80, 0x00, 0x00, 0x0C, 0xEC] {
            //     return Err(format_err!("Bad ACK received: {:02x?}", ack));
            // }
        }

        Ok(())
    }
    */

    fn append_open_new_application_request(fctr: FCTR, out: &mut Vec<u8>) {
        Self::append_request(
            fctr,
            Command::OpenApplication,
            0, // Initialize a clean app context
            UNIQUE_APP_ID,
            out,
        )
    }

    /// Executes a full request on the device along with waiting for the
    /// response to be received.
    async fn call(&mut self, command: Command, param: u8, data: &[u8]) -> Result<Vec<u8>> {
        let frame_counter = self.next_frame_counter;
        self.next_frame_counter += 1;

        let last_frame_counter = (frame_counter + 4 - 1) % 4;

        // Step 1: Issue request
        {
            // TODO: Avoid allocating vecs and instead use a shared instance wide buffer.
            let mut request = vec![DATA_REG];
            Self::append_request(
                FCTR::DataFrameWithAck {
                    frame_num: frame_counter,
                    ack_num: last_frame_counter,
                },
                command,
                param,
                data,
                &mut request,
            );

            if command == Command::OpenApplication {
                assert_eq!(
                    &request,
                    &[
                        0x80, 0x03, 0x00, 0x15, 0x00, 0x70, 0x00, 0x00, 0x10, 0xD2, 0x76, 0x00,
                        0x00, 0x04, 0x47, 0x65, 0x6E, 0x41, 0x75, 0x74, 0x68, 0x41, 0x70, 0x70,
                        0x6C, 0x04, 0x1A
                    ]
                );
            } else if command == Command::GetDataObject {
                assert_eq!(
                    &request,
                    &[
                        0x80, 0x04, 0x00, 0x0B, 0x00, 0x01, 0x00, 0x00, 0x06, 0xE0, 0xC2, 0x00,
                        0x00, 0x00, 0x64, 0xF0, 0x9F
                    ]
                );
            }

            self.write(&request).await?;
        }

        // Step 2: (optionally) Read ACK
        //
        // Some commands may immediately have a response data frame without emitting an
        // ack.
        //
        // We expect the state to be roughly 'C8 80 00 05'
        // - busy: true
        // - response_ready: true,
        // - read_length: 5,
        {
            let state = self.wait_for_response_ready().await?;
            if state.busy {
                let mut data = vec![0u8; state.read_length];
                self.write(&[DATA_REG]).await?;
                self.read(&mut data).await?;

                let ack_frame = Self::parse_frame(&data)?;
                if ack_frame.fctr
                    != (FCTR::ControlFrameWithAck {
                        ack_num: frame_counter,
                    })
                {
                    return Err(format_err!(
                        "Wrong FCTR in received ACK frame. Desynced? (frame_counter: {}) : Received FCTR: {:?}",
                        frame_counter,
                        ack_frame.fctr
                    ));
                }

                if !ack_frame.data.is_empty() {
                    return Err(err_msg("Expected no data in received ACK frame"));
                }
            }
        }

        // Step 3: Read actual response.
        let state = self.wait_for_response_ready().await?;
        if state.busy {
            return Err(err_msg(
                "Device should not be busy after response is generated",
            ));
        }

        let mut data = vec![0u8; state.read_length];
        self.write(&[DATA_REG]).await?;
        self.read(&mut data).await?;

        let resp_frame = Self::parse_frame(&data)?;
        if resp_frame.fctr
            != (FCTR::DataFrameWithAck {
                frame_num: frame_counter,
                ack_num: frame_counter,
            })
        {
            return Err(err_msg("Wrong FCTR in received response frame. Desynced?"));
        }

        // Step 4: Send back an ACK frame
        {
            // TODO: This should always be fixed length so doesn't need to vec![]
            let mut response_ack = vec![DATA_REG];
            Self::append_frame(
                FCTR::ControlFrameWithAck {
                    ack_num: frame_counter,
                }
                .encode(),
                |_| {},
                &mut response_ack,
            );
            self.write(&response_ack).await?;
        }

        // Step 5: Parsing the response.

        // Strip the PCTR
        let raw_resp = {
            if resp_frame.data.len() < 1 || resp_frame.data[0] != 0x00 {
                return Err(err_msg("Missing or non-zero PCTR in response"));
            }

            &resp_frame.data[1..]
        };

        let (status, response_data) = Self::parse_response(raw_resp)?;
        if status != TrustMErrorCode::NoError {
            return Err(format_err!("Error returned in response: {:?}", status));
        }

        Ok(response_data.to_vec())
    }

    async fn write(&mut self, data: &[u8]) -> Result<()> {
        let start = Instant::now();
        loop {
            let r = self.device.write(data).await;

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

    async fn read(&mut self, data: &mut [u8]) -> Result<()> {
        self.device.read(data).await?;
        std::thread::sleep(GUARD_TIME);
        Ok(())
    }

    /// Waits until there is data that can be read from the device.
    async fn wait_for_response_ready(&mut self) -> Result<I2CState> {
        for _ in 0..10 {
            let state = self.get_i2c_state().await?;
            if state.response_ready {
                if state.read_length == 0 {
                    return Err(err_msg("Response ready but has zero bytes"));
                }

                return Ok(state);
            }
        }

        Err(err_msg("Took too long for a response to be available"))
    }

    async fn get_i2c_state(&mut self) -> Result<I2CState> {
        self.write(&[I2C_STATE_REG]).await?;

        let mut buf = [0u8; 4];
        self.read(&mut buf).await?;

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

    async fn get_guard_time(&mut self) -> Result<[u8; 4]> {
        self.write(&[GUARD_TIME_REG]).await?;

        let mut buf = [0u8; 4];
        self.read(&mut buf).await?;

        Ok(buf)
    }

    /// Appends an application layer request to the given buffer.
    fn append_request(fctr: FCTR, command: Command, param: u8, data: &[u8], out: &mut Vec<u8>) {
        Self::append_frame(
            fctr.encode(),
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

    /// The format of a response is:
    /// - Sta: 1 bytes : Response status code
    /// - Undef (1 byte) : Undefined
    /// - Len (2 bytes)
    /// - Data (len bytes)
    fn parse_response<'a>(mut input: &'a [u8]) -> Result<(TrustMErrorCode, &'a [u8])> {
        let status = TrustMErrorCode::from_value(parse_next!(input, be_u8));
        let undef = parse_next!(input, be_u8);
        // if undef != 0 {
        //     return Err(err_msg("Non-zero undefined byte received"));
        // }

        let len = parse_next!(input, be_u16);
        let data = parse_next!(input, take_exact(len as usize));

        if !input.is_empty() {
            return Err(err_msg("Extra data after end of response"));
        }

        Ok((status, data))
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

    fn parse_frame<'a>(mut input: &'a [u8]) -> Result<FrameRef<'a>> {
        if input.len() < 5 {
            return Err(err_msg("Frame is too short"));
        }

        let expected_fcs = Self::crc(&input[0..(input.len() - 2)]);

        let fctr = parse_next!(input, be_u8);
        let len = parse_next!(input, be_u16);
        let data = parse_next!(input, take_exact(len as usize));
        let fcs = parse_next!(input, take_exact(2));

        if !input.is_empty() {
            return Err(err_msg("Extra data after end of frame"));
        }

        if fcs != &expected_fcs[..] {
            return Err(err_msg("Invalid checksum in frame"));
        }

        Ok(FrameRef {
            fctr: FCTR::decode(fctr)?,
            data,
        })
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

struct FrameRef<'a> {
    fctr: FCTR,
    data: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FCTR {
    /// Current frame has number N and is ACKing frame M.
    DataFrameWithAck {
        frame_num: u8,
        ack_num: u8,
    },

    /// Current frame has number N and is NAKing frame M.
    DataFrameWithNak {
        frame_num: u8,
        nak_num: u8,
    },

    /// Control frame 0 which is ACKing prior frame N.
    ControlFrameWithAck {
        ack_num: u8,
    },

    /// Control frame 0 which is NAKing prior frame N.
    ControlFrameWithNak {
        nak_num: u8,
    },

    ResetFrameCounter,
}

impl FCTR {
    pub fn decode(value: u8) -> Result<Self> {
        let ftype = value >> 7;
        let seqctr = (value >> 5) & 0b11;
        let frame_num = (value >> 2) & 0b11;
        let ack_num = value & 0b11;

        if ftype == 1 {
            // Control frame
            if frame_num != 0 {
                return Err(err_msg("Control frames should all have number 0"));
            }

            return Ok(match seqctr {
                0b00 => Self::ControlFrameWithAck { ack_num },
                0b01 => Self::ControlFrameWithNak { nak_num: ack_num },
                0b10 => {
                    if ack_num != 0 {
                        return Err(err_msg("ACK not allowed with reset frame counter frame"));
                    }
                    Self::ResetFrameCounter
                }
                _ => return Err(err_msg("Unsupported seqctr")),
            });
        }

        Ok(match seqctr {
            0b00 => Self::DataFrameWithAck { frame_num, ack_num },
            0b01 => Self::DataFrameWithNak {
                frame_num,
                nak_num: ack_num,
            },
            _ => return Err(err_msg("Unsupported seqctr")),
        })
    }

    pub fn encode(&self) -> u8 {
        fn inner(is_control: bool, seqctr: u8, frame_num: u8, ack_num: u8) -> u8 {
            (if is_control { 1 << 7 } else { 0 }) | (seqctr << 5) | (frame_num << 2) | ack_num
        }

        match *self {
            FCTR::DataFrameWithAck { frame_num, ack_num } => inner(false, 0b00, frame_num, ack_num),
            FCTR::DataFrameWithNak { frame_num, nak_num } => inner(false, 0b01, frame_num, nak_num),
            FCTR::ControlFrameWithAck { ack_num } => inner(true, 0b00, 0, ack_num),
            FCTR::ControlFrameWithNak { nak_num } => inner(true, 0b01, 0, nak_num),
            FCTR::ResetFrameCounter => inner(true, 0b10, 0, 0),
        }
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

#[derive(Clone, Copy, PartialEq)]
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

    /// Testing against golden values in section 10.1.2 ('Issue
    /// OpenApplication') of the SLS32AIA datasheet.
    #[test]
    fn issue_open_application_test() {
        // Step 1: Send OpenApplication command.
        {
            let mut request = vec![];
            TrustM::append_open_new_application_request(FCTR::decode(0x03).unwrap(), &mut request);

            assert_eq!(
                &request,
                &[
                    0x03, 0x00, 0x15, 0x00, 0x70, 0x00, 0x00, 0x10, 0xD2, 0x76, 0x00, 0x00, 0x04,
                    0x47, 0x65, 0x6E, 0x41, 0x75, 0x74, 0x68, 0x41, 0x70, 0x70, 0x6C, 0x04, 0x1A
                ]
            );
        }

        // Step 6: Control frame 0, Ack data frame 0
        {
            let mut ack = vec![];
            TrustM::append_frame(
                FCTR::ControlFrameWithAck { ack_num: 0 }.encode(),
                |_| {},
                &mut ack,
            );

            assert_eq!(&ack, &[0x80, 0x00, 0x00, 0x0C, 0xEC]);
        }
    }

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
        TrustM::append_open_new_application_request(FCTR::decode(0x03).unwrap(), &mut frame);
        assert_eq!(&frame, &expected_frame);
    }
}
