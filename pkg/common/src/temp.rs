// Utilities for creating temporary files.

use std::path::{Path, PathBuf};
use std::string::ToString;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::*;

pub struct TempDir {
    dir: PathBuf,
}

impl TempDir {
    pub fn create() -> Result<Self> {
        loop {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = Path::new("/tmp/dacha").join(time.to_string());

            let _ = std::fs::create_dir("/tmp/dacha");

            if let Err(e) = std::fs::create_dir(&path) {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    continue;
                }

                return Err(e.into());
            }

            return Ok(Self { dir: path });
        }
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}
