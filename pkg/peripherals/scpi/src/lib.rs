#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::time::Duration;

use common::io::{Writeable, Readable};
use common::errors::*;
use net::tcp::TcpStream;


#[derive(Debug, Clone)]
pub struct SCPIIdentity {
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub firmware_rev: String
}

impl SCPIIdentity {
    pub fn instrument_type(&self) -> InstrumentType {
        if self.manufacturer == "Siglent Technologies" {
            if self.model.starts_with("SPD") {
                return InstrumentType::PowerSupply;
            }

            if self.model.starts_with("SDM") {
                return InstrumentType::Multimeter;
            }
        }

        InstrumentType::Unknown
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InstrumentType {
    Unknown,
    Multimeter,
    PowerSupply
}

#[derive(Debug, Clone)]
pub struct PSUChannelMeasurement {
    pub voltage: f32,
    pub current: f32
}


pub struct SCPIClient {
    client: TcpStream,
}

impl SCPIClient {
    pub async fn create(addr: &str) -> Result<Self> {
        let mut client = TcpStream::connect(format!("{}:5025", addr).parse()?).await?;

        let mut inst = Self {
            client
        };

        Ok(inst)
    }

    pub async fn run_command(&mut self, cmd: &str) -> Result<String> {
        if cmd.contains("\n") {
            return Err(err_msg("Invalid command"));
        }

        let request = format!("{}\n", cmd);        
        self.client.write_all(request.as_bytes()).await?;

        let mut buf = vec![0u8; 128];
        let n = self.client.read(&mut buf).await?;

        let res = std::str::from_utf8(&buf[0..n])?;
        
        let res = res.strip_suffix("\n")
            .ok_or_else(|| err_msg("Response doesn't end up a new line"))?;

        Ok(res.to_string())
    }

    pub async fn run_command_noreply(&mut self, cmd: &str) -> Result<()> {
        if cmd.contains("\n") {
            return Err(err_msg("Invalid command"));
        }

        let request = format!("{}\n", cmd);        
        self.client.write_all(request.as_bytes()).await?;

        Ok(())
    }

    pub async fn run_binary_command(
        &mut self,
        cmd: &str,
        response_size: Option<usize>
    ) -> Result<Vec<u8>> {
        if cmd.contains("\n") {
            return Err(err_msg("Invalid command"));
        }

        let request = format!("{}\n", cmd);        
        self.client.write_all(request.as_bytes()).await?;
        
        let mut buf = vec![];

        if let Some(response_size) = response_size {
            buf.resize(response_size, 0);
            self.client.read_exact(&mut buf).await?;
        } else {
            self.read_binary_data_timeout(&mut buf).await?;
        }

        Ok(buf)
    }

    async fn read_binary_data_timeout(&mut self, out: &mut Vec<u8>) -> Result<()> {
        loop {
            let mut chunk = vec![0u8; 4096];
            let n = match executor::timeout(Duration::from_millis(100), self.client.read(&mut chunk)).await {
                Ok(v) => v?,
                Err(_) => break
            };

            if n == 0 {
                break;
            }

            out.extend_from_slice(&chunk[..n]);   
        }

        Ok(())
    }

    /// Known identity strings:
    /// Siglent Technologies,SPD3303X-E,SPD3XIDQ5R5553,1.01.01.02.07R2,V3.0
    /// Siglent Technologies,SDM3055,SDM35GBX5R0872,1.01.01.22R1
    pub async fn identity(&mut self) -> Result<SCPIIdentity> {
        let data = self.run_command("*IDN?").await?;

        let parts = data.splitn(4, ",").collect::<Vec<&str>>();
        if parts.len() != 4 {
            return Err(err_msg("Invalid identity string"));
        }

        Ok(SCPIIdentity {
            manufacturer: parts[0].to_string(),
            model: parts[1].to_string(),
            serial: parts[2].to_string(),
            firmware_rev: parts[3].to_string()
        })
    }

    pub async fn check_instrument_type(&mut self, expected_type: InstrumentType) -> Result<()> {
        let t = self.identity().await?.instrument_type();

        if t != expected_type {
            return Err(format_err!("Wrong instrument type. Expected {:?}. Found: {:?}", expected_type, t));
        }

        Ok(())
    }

    pub async fn measure_temp_ktype(&mut self) -> Result<f32> {
        let data = self.run_command("MEAS:TEMP? THER,KITS90").await?;
        Ok(data.parse::<f32>()?)
    }

    pub async fn measure_psu_ch1(&mut self) -> Result<PSUChannelMeasurement> {
        let voltage = self.run_command("MEAS:VOLT? CH1").await?.parse::<f32>()?;
        let current = self.run_command("MEAS:CURR? CH1").await?.parse::<f32>()?;
        
        Ok(PSUChannelMeasurement {
            voltage, current
        })
    }

    /// For multimeters
    pub async fn measure_voltage(&mut self) -> Result<f32> {
        let data = self.run_command("MEAS:VOLT:DC?").await?;
        Ok(data.parse::<f32>()?)
    }

}

