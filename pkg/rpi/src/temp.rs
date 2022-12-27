/*
Can read temperature from /sys/class/thermal/thermal_zone0/temp

Units are 1000 per degree C

*/

use common::errors::*;
use common::failure::ResultExt;
use common::futures::AsyncReadExt;
use common::io::Readable;
use file::LocalFile;

pub struct CPUTemperatureReader {
    file: LocalFile,
}

impl CPUTemperatureReader {
    pub async fn create() -> Result<Self> {
        let file = LocalFile::open("/sys/class/thermal/thermal_zone0/temp").with_context(|e| {
            format!("While opening /sys/class/thermal/thermal_zone0/temp: {}", e)
        })?;
        Ok(Self { file })
    }

    /// Returns the temperature in degrees Celsius
    pub async fn read(&mut self) -> Result<f64> {
        self.file.seek(0);

        // NOTE: Will end in a '\n'.
        let mut value = String::new();
        self.file.read_to_string(&mut value).await?;

        let milli_degrees = value.trim_end().parse::<f64>()?;

        Ok(milli_degrees / 1000.0)
    }
}
