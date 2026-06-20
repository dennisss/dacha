use common::errors::*;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Model {
    CM4,
    CM5,
    Unknown(String)
}

impl Model {
    /// NOTE: This is the same as the "Model" field in /proc/cpuinfo
    pub async fn get() -> Result<Self> {
        let name = file::read_to_string("/sys/firmware/devicetree/base/model").await?;

        if !name.contains("Raspberry Pi") {
            return Ok(Self::Unknown(name));
        }

        if name.contains("Compute Module 4") {
            return Ok(Self::CM4);
        }

        if name.contains("Compute Module 5") {
            return Ok(Self::CM5);
        }

        Ok(Self::Unknown(name))
    }
}
