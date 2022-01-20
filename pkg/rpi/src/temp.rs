/*
Can read temperature from /sys/class/thermal/thermal_zone0/temp

Units are 1000 per degree C

*/

use common::async_std::fs::File;
use common::async_std::io::prelude::SeekExt;
use common::async_std::io::SeekFrom;
use common::errors::*;
use common::futures::AsyncReadExt;

pub struct CPUTemperatureReader {
    file: File,
}

impl CPUTemperatureReader {
    pub async fn create() -> Result<Self> {
        let file = File::open("/sys/class/thermal/thermal_zone0/temp").await?;
        Ok(Self { file })
    }

    /// Returns the temperature in degrees Celsius
    pub async fn read(&mut self) -> Result<f64> {
        self.file.seek(SeekFrom::Start(0)).await?;

        // NOTE: Will end in a '\n'.
        let mut value = String::new();
        self.file.read_to_string(&mut value).await?;

        let milli_degrees = value.trim_end().parse::<f64>()?;

        Ok(milli_degrees / 1000.0)
    }
}
