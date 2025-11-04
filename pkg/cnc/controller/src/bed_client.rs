use std::time::Duration;

use common::errors::*;
use common::io::{Readable, Writeable};
use peripherals::serial::SerialPort;
use file::LocalPath;
use parsing::cstruct::*;
use cnc_controller_proto::cnc::BedClientConfig;
use electronics::*;

const BAUD_RATE: usize = 115_200;
const ADDRESS: u8 = 0xAB;
const TIMEOUT: Duration = Duration::from_millis(100);

pub struct BedClient {
    port: SerialPort,
    last_sequence: u8,
    options: BedClientOptions,
}

pub struct BedClientOptions {
    pub config: BedClientConfig,
}

#[derive(Debug, Default)]
pub struct Response {
    pub chip_temperature: f32,
    pub sheet_temperature: f32,
    pub bed_temperature: f32,
    pub fan_speed: usize,
}

// TODO: Must match the C code.
#[repr(packed)]
struct RequestPacket {
    length: u8,
    address: u8,
    sequence: u8,
    desired_fan_speed: u8,
    desired_led_color: u32,
    checksum: u8
}

// TODO: Must match the C code.
#[derive(Debug, Default)]
#[repr(packed)]
struct ResponsePacket {
    length: u8,
    address: u8,
    sequence: u8,

    chip_temperature: u16,

    sheet_temperature: u16,

    bed_temperature: u16,
    fan_speed: u16,
    checksum: u8
}


impl BedClient {
    pub fn create(path: &LocalPath, options: BedClientOptions) -> Result<Self> {
        let port = SerialPort::open(path, BAUD_RATE)?;
        Ok(Self {
            port,
            last_sequence: 0,
            options,
        })
    }

    // TODO: If we ever need to retry, do tcflush(TCIOFLUSH)
    // Read also https://stackoverflow.com/questions/13013387/clearing-the-serial-ports-buffer

    pub async fn request(&mut self, desired_fan_speed: u8, desired_led_color: u32) -> Result<Response> {
        let sequence = self.last_sequence.wrapping_add(1);
        self.last_sequence = sequence;

        let mut request_packet = RequestPacket {
            length: 0,
            address: ADDRESS,
            sequence,
            desired_fan_speed,
            desired_led_color,
            checksum: 0
        };

        let length = unsafe { serialize_cstruct_raw(&request_packet) }.len();
        request_packet.length = length as u8;

        let sum = crypto::checksum::crc8::crc8(&unsafe { serialize_cstruct_raw(&request_packet) }[0..(length - 1)]);
        request_packet.checksum = sum;

        let request_data = unsafe {
            serialize_cstruct_raw(&request_packet)
        };
        
        let response_data = executor::timeout(TIMEOUT, self.request_impl(request_data)).await??;
        if response_data.len() < 4 {
            return Err(err_msg("Too few response bytes"));
        }

        let expected_sum = crypto::checksum::crc8::crc8(&response_data[0..response_data.len() - 1]);
        if expected_sum != response_data[response_data.len() - 1] {
            return Err(err_msg("Wrong checksum in response"));
        }

        if response_data.len() != core::mem::size_of::<ResponsePacket>() {
            return Err(err_msg("Wrong response length"));
        }

        let mut response_packet = ResponsePacket::default();
        unsafe { parse_cstruct_raw(&response_data[..], &mut response_packet).unwrap(); }

        if response_packet.sequence != self.last_sequence {
            return Err(err_msg("Response has wrong sequence"));
        }

        if response_packet.address != ADDRESS {
            return Err(err_msg("Response has wrong address"));
        }

        // println!("{:?}", response_packet);

        let bed_res = undivide_voltage_lower(1.0, self.calibrated_value(response_packet.bed_temperature),
            self.options.config.bed_temp_resistor());
        let sheet_res = undivide_voltage_lower(1.0, self.calibrated_value(response_packet.sheet_temperature),
            self.options.config.sheet_temp_resistor());

        // println!("Sheet Res: {}", sheet_res);
        
        Ok(Response {
            chip_temperature: (response_packet.chip_temperature as f32) * self.options.config.chip_temp_calibration() - 273.15,

            sheet_temperature: PT1000::default().resistance_to_temperature(sheet_res).unwrap(),
            bed_temperature: PT1000::default().resistance_to_temperature(bed_res).unwrap(),

            fan_speed: response_packet.fan_speed as usize
        })
    }

    fn calibrated_value(&self, raw_value: u16) -> f32 {
        let adc_max = ((1u32 << 13) - 1) as f32;
        let v = (raw_value as f32) / adc_max;

        self.options.config.calibration_a() * v + self.options.config.calibration_b()
    }

    async fn request_impl(&mut self, request_data: &[u8]) -> Result<Vec<u8>> {
        self.port.write_all(request_data).await?;

        // This is intentionally very big so that we also to consume any stray bytes in case there is corruption.
        let mut response_buffer = vec![0u8; 512];

        // Read the echo'ed bytes.
        self.port.read_exact(&mut response_buffer[0..request_data.len()]).await?;

        if &response_buffer[0..request_data.len()] != request_data {
            return Err(err_msg("Wrong echo'ed bytes"));
        }

        let mut total_received = 0;

        loop {
            let n = self.port.read(&mut response_buffer[total_received..]).await?;
            if n == 0 {
                return Err(err_msg("Hit end of serial port"));
            }

            total_received += n;

            if total_received > (response_buffer[0] as usize) {
                return Err(err_msg("Received more than a complete packet"));
            }

            if total_received == (response_buffer[0] as usize) {
                break;
            }
        }

        response_buffer.truncate(response_buffer[0] as usize);

        Ok(response_buffer)
    }
}

