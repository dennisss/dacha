use common::errors::*;
use sys::utsname::*;

use crate::proto::*;

pub fn current_platform() -> Result<Platform> {
    let mut platform = Platform::default();

    let name = UtsName::read()?;

    platform.set_architecture(match name.machine() {
        "x86_64" => Architecture::AMD64,
        "aarch64" => Architecture::AArch64,
        v @ _ => {
            return Err(format_err!("Unknown machine type: {}", v));
        }
    });

    platform.set_os(match name.sysname() {
        "Linux" => Os::LINUX,
        v @ _ => {
            return Err(format_err!("Unknown sysname: {}", v));
        }
    });

    Ok(platform)
}
