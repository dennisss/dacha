

/*


0xA0 / 0xA1 addresses used for EDID

0x60 is the segment pointer to different 256 byte segmenets.


0b 1010 0001
*/

use common::errors::*;
use crate::i2c::I2CDevice;

fn compute_checksum(data: &[u8]) -> u8 {
    let mut sum = 0;
    for b in data.iter() {
        sum ^= *b;
    }

    sum
}

fn push_checksum(data: &mut Vec<u8>) {
    let mut sum = 0;
    for b in data.iter() {
        sum ^= *b;
    }

    data.push(sum);
}

// data[0] <- Destination addr
// data[1] <- Source addr
// data[2] <- 

// CHK' computed using 0x50 virtual host address.

pub struct DDCDevice {
    i2c: I2CDevice
}

impl DDCDevice {
    pub fn open(path: &str) -> Result<Self> {
        let i2c = I2CDevice::open(path)?;
        Ok(Self { i2c })
    }

    pub fn read_edid(&mut self) -> Result<()> {
        

        self.i2c.write(0x60 >> 1, &[0])?;

        let mut data = vec![0; 256];
        self.i2c.read(0x50, &mut data)?;

        println!("EXTENSION FLAG: {:?}", data[125]);

        println!("{:?}", data);
        // println!("{:?}", common::bytes::Bytes::from(data));

        Ok(())
    }

    pub fn get_capabilities(&mut self) -> Result<String> {
        let mut full_data = vec![];
    
        let mut offset: u16 = 0;
        loop {
            println!("READ AT OFFSET: {}", offset);
    
            // TODO: Check the first byte of the request and response.
    
            let mut req = vec![0x6E, 0x51, 0x83, 0xF3];
            req.extend_from_slice(&offset.to_be_bytes());
            req.push(compute_checksum(&req));
            self.i2c.write(req[0] >> 1, &req[1..])?;
    
            std::thread::sleep(std::time::Duration::from_millis(50));
    
            let mut resp = [0u8; 32 + 7];
            resp[0] = 0x6E; // TODO: Check this!!
            self.i2c.read(resp[0] >> 1, &mut resp[1..])?;
    
            // resp[1] is the source address
            resp[1] = 0x50;
    
            let resp_len = resp[2] & 0x7f;
    
            let resp_end_offset = (3 + resp_len) as usize; // TODO: Check this.
    
            if resp[3] != 0xE3 {
                return Err(err_msg("Wrong reply code"));
            }
    
            let resp_offset = ((resp[4] as u16) << 8) | (resp[5] as u16);
            if offset != resp_offset {
                return Err(err_msg("Received different offset than requested"));
            }
    
            let resp_data = &resp[6..resp_end_offset];
    
            let resp_checksum = resp[resp_end_offset];
    
            let expected_checksum = compute_checksum(&resp[0..resp_end_offset]);
            if resp_checksum != expected_checksum {
                return Err(format_err!("Invalid response checksum {:2x} != {:2x}", resp_checksum, expected_checksum));
            }
        
            let mut null_terminator = None;
            for i in 0..resp_data.len() {
                if resp_data[i] == 0 {
                    null_terminator = Some(i);
                    break;
                }
            }
    
            full_data.extend_from_slice(&resp_data);
            offset += resp_data.len() as u16;
    
            if null_terminator.is_some() || resp_data.len() == 0 {
                break;
            }
        }

        let ret = String::from_utf8(full_data)?;
        Ok(ret)
    }

    pub fn get_vcp_feature(&mut self, code: u8) -> Result<Feature> {
        let mut req = vec![0x6E, 0x51, 0x82, 0x01, code];
        req.push(compute_checksum(&req));
        self.i2c.write(req[0] >> 1, &req[1..])?;

        std::thread::sleep(std::time::Duration::from_millis(40));


        let mut resp = [0u8; 12];
        resp[0] = 0x6E;
        self.i2c.read(resp[0] >> 1, &mut resp[1..])?;

        resp[1] = 0x50;

        if resp[2] != 0x88 || resp[3] != 0x02 {
            return Err(err_msg("Invalid response"));
        }

        let rc = resp[4];
        if rc == 0 {
            // No error
        } else if rc == 1 {
            return Err(err_msg("Unsupported feature code"));
        } else {
            return Err(err_msg("Unknown error"));
        }

        if resp[5] != code {
            return Err(err_msg("Received non-requested code"));
        }

        let type_code = resp[6];
        
        let typ = match type_code {
            0 => FeatureType::SetParameter,
            1 => FeatureType::Momentary,
            _ => { return Err(err_msg("Unknown feature type")); }
        };

        let max_value = u16::from_be_bytes(*array_ref![resp, 7, 2]);
        let current_value = u16::from_be_bytes(*array_ref![resp, 9, 2]);

        let resp_checksum = resp[resp.len() - 1];
        let expected_checksum = compute_checksum(&resp[0..(resp.len() - 1)]);
        if resp_checksum != expected_checksum {
            return Err(format_err!("Invalid response checksum {:2x} != {:2x}", resp_checksum, expected_checksum));
        }

        Ok(Feature {
            typ, max_value, current_value
        })
    }

    pub fn set_vcp_feature(&mut self, code: u8, value: u16) -> Result<()> {
        let mut req = vec![0x6E, 0x51, 0x84, 0x03, code];
        req.extend_from_slice(&value.to_be_bytes());
        req.push(compute_checksum(&req));
        self.i2c.write(req[0] >> 1, &req[1..])?;

        std::thread::sleep(std::time::Duration::from_millis(40));

        Ok(())
    }
}

#[derive(Debug)]
pub enum FeatureType {
    SetParameter,
    Momentary
}

#[derive(Debug)]
pub struct Feature {
    pub typ: FeatureType,
    pub max_value: u16,
    pub current_value: u16
}