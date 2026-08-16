use common::snake_to_camel_case;
use common::errors::*;

use crate::{LocalPathBuf, LocalPath};


#[cfg(target_os = "linux")]
pub fn local_app_data_dir(app_name: &str) -> Result<LocalPathBuf> {
    let home = std::env::var("HOME")?;
    Ok(LocalPath::new(&home).join(format!(".local/share/{}", app_name)))
}

#[cfg(target_os = "windows")]
pub fn local_app_data_dir(app_name: &str) -> Result<LocalPathBuf> {
    let dir = std::env::var("LOCALAPPDATA")?;
    let name = snake_to_camel_case(app_name);
    Ok(LocalPath::new(&dir).join(name))
}

#[cfg(target_os = "macos")]
pub fn local_app_data_dir(app_name: &str) -> Result<LocalPathBuf> {
    let home = std::env::var("HOME")?;
    let name = snake_to_camel_case(app_name);
    Ok(LocalPath::new(&home).join(format!("Library/Application Support/{}", name)))
}
