use alloc::string::{ToString, String};
use alloc::vec::Vec;

use file::{LocalPath, LocalPathBuf};
use common::errors::*;


#[derive(Debug, Clone)]
pub struct SASExpander {
    pub name: String,

    pub device_path: LocalPathBuf,

    pub vendor_id: String,

    pub product_id: String,

    pub product_rev: String,

    pub sas_address: String,

    pub ports: Vec<SASPort>,
}

#[derive(Debug, Clone)]
pub struct SASPort {
    pub path: LocalPathBuf,
    pub phys: Vec<usize>,
    pub inner_device_paths: Vec<LocalPathBuf>,
}

impl SASExpander {
    pub async fn list() -> Result<Vec<Self>> {
        let mut out = vec![];

        let path = LocalPath::new("/sys/class/sas_expander");
        if !file::exists(path).await? {
            return Ok(out);
        }

        let devices = file::read_dir(path)?;
        for entry in devices {
            let name = entry.name().to_string();
            let dir = path.join(entry.name());

            let device_path = file::realpath(dir.join("device")).await?;

            let vendor_id = Self::read_property(dir.join("vendor_id")).await?;
            let product_id = Self::read_property(dir.join("product_id")).await?;
            let product_rev = Self::read_property(dir.join("product_rev")).await?;

            let sas_address = Self::read_property(
                dir.join("device/sas_device").join(&name).join("sas_address")).await?;

            let mut ports = vec![];

            for entry in file::read_dir(&device_path)? {
                if entry.typ() != file::FileType::Directory {
                    continue;
                }

                let dir = device_path.join(entry.name());

                if let Some(port_id) = entry.name().strip_prefix("port-") {
                    
                    let mut inner_device_paths = vec![];

                    let mut phys = vec![];
                    for entry in file::read_dir(&dir)? {
                        // Names will look like "phy-8:0:20"
                        if entry.name().starts_with("phy-") {
                            let num = entry.name().rsplit_once(":")
                                .ok_or_else(|| err_msg("Unknown phy name pattern"))?.1
                                .parse::<usize>()?;
                            phys.push(num);
                        }

                        if entry.name().starts_with("end_device-") {
                            Self::traverse_end_device(&dir.join(entry.name()), &mut inner_device_paths).await?;
                        }
                    }

                    ports.push(SASPort {
                        path: dir,
                        phys,
                        inner_device_paths,
                    });                    
                }
            }

            out.push(Self {
                name,
                device_path,
                vendor_id,
                product_id,
                product_rev,
                sas_address,
                ports,
            });
        }

        Ok(out)
    }

    async fn traverse_end_device(path: &LocalPath, inner_device_paths: &mut Vec<LocalPathBuf>) -> Result<()> {
        for entry in file::read_dir(path)? {

            let path = path.join(entry.name());

            if let Some(target_id) = entry.name().strip_prefix("target") {
                for entry in file::read_dir(&path)? {
                    if entry.typ() != file::FileType::Directory {
                        continue;
                    }

                    if entry.name().starts_with(target_id) {
                        inner_device_paths.push(path.join(entry.name()));
                    }

                }
            }
        }

        Ok(())
    }

    async fn read_property<P: AsRef<LocalPath>>(path: P) -> Result<String> {
        Ok(file::read_to_string(path).await?.trim().to_string())
    }
}