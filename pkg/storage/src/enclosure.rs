use alloc::string::{ToString, String};
use alloc::vec::Vec;

use file::{LocalPath, LocalPathBuf};
use common::errors::*;

#[derive(Debug, Clone)]
pub struct EnclosureEntry {
    pub name: String,

    pub device_path: LocalPathBuf,
}

impl EnclosureEntry {
    pub async fn list() -> Result<Vec<Self>> {
        let mut out = vec![];

        let path = LocalPath::new("/sys/class/enclosure");
        let devices = file::read_dir(path)?;
        for entry in devices {
            let name = entry.name().to_string();
            let dir = path.join(entry.name());

            let device_path = file::realpath(dir.join("device")).await?;

            out.push(Self {
                name,
                device_path
            })
        }

        Ok(out)
    }
}
