mod options;
mod dir;
#[cfg(target_os = "linux")]
mod file;
#[cfg(target_os = "linux")]
mod metadata;
#[cfg(target_os = "linux")]
mod path;
#[cfg(target_os = "linux")]
mod device_num;

pub use options::*;
#[cfg(target_os = "linux")]
pub use self::file::*;
pub use dir::*;
#[cfg(target_os = "linux")]
pub use metadata::*;
#[cfg(target_os = "linux")]
pub use path::*;

#[cfg(target_os = "linux")]
pub use device_num::*;


#[cfg(target_os = "linux")]
mod watcher;
#[cfg(target_os = "linux")]
pub use watcher::*;

#[cfg(not(target_os = "linux"))]
mod watcher_windows;
#[cfg(not(target_os = "linux"))]
pub use watcher_windows::*;


#[cfg(not(target_os = "linux"))]
pub type LocalPath = std::path::Path;
#[cfg(not(target_os = "linux"))]
pub type LocalPathBuf = std::path::PathBuf;


#[cfg(not(target_os = "linux"))]
mod file_blocking;
#[cfg(not(target_os = "linux"))]
pub use file_blocking::*;