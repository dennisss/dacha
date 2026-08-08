// Utilities for creating temporary files.

use std::string::ToString;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;

use common::errors::*;

use crate::{LocalPath, LocalPathBuf};

/// A temporary directory which an application can help for writing intermediate
/// files.
///
/// Each instance of a TempDir will provide a distinct exclusively owned
/// (informally) directory which minimally doesn't conflict with other TempDir
/// instances.
///
/// The contents of the directory will be deleted after the TempDir is dropped.
/// If a graceful drop doesn't occur, the system will eventually clean up the
/// directory.
pub struct TempDir {
    dir: LocalPathBuf,
}

impl TempDir {
    pub fn create() -> Result<Self> {
        // Normally '/tmp'
        let sys_dir = std::env::temp_dir();
        let project_dir = sys_dir.join("dacha");

        if !std::fs::exists(&project_dir)? {
            std::fs::create_dir(&project_dir)?;

            // TODO: Make permissions scoped to one user for better security?
            #[cfg(target_os = "linux")]
            {
                // If the temp directory is created for the first time by 'root', we need to make sure that regular users can still use it.
                let mut perms = std::fs::metadata(&project_dir)?.permissions();
                perms.set_mode(0o777);
                std::fs::set_permissions(&project_dir, perms)?;
            }
        }

        loop {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();

            let path = LocalPath::new(project_dir.to_str().unwrap()).join(time.to_string());

            if let Err(e) = std::fs::create_dir(&path) {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    continue;
                }

                return Err(e.into());
            }

            return Ok(Self { dir: path });
        }
    }

    pub fn path(&self) -> &LocalPath {
        &self.dir
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::ToOwned;
    use common::errors::*;

    use crate::{LocalFile, LocalFileOpenOptions};

    use super::*;

    #[testcase]
    async fn temp_dir_works() -> Result<()> {
        let tmpdir = TempDir::create()?;
        let tmpdir2 = TempDir::create()?;

        // assert!(tmpdir.path().as_str().starts_with("/tmp/"));

        // Verify we get distinct directories.
        assert!(tmpdir.path() != tmpdir2.path());

        // The directories should now exist.
        assert!(crate::exists(tmpdir.path()).await?);
        assert!(crate::exists(tmpdir2.path()).await?);

        // Verify the first temp dir is writeable.
        let filepath = tmpdir.path().join("test_file");
        crate::write(&filepath, "testing123").await?;
        assert!(crate::exists(&filepath).await?);
        assert_eq!(crate::read_to_string(&filepath).await?, "testing123");

        // Verify dropping the second dir deletes the second dir but doesn't impact the
        // first one.
        let tmpdir2_path = tmpdir2.path().to_owned();
        drop(tmpdir2);
        assert!(!crate::exists(&tmpdir2_path).await?);
        assert_eq!(crate::read_to_string(&filepath).await?, "testing123");

        drop(tmpdir);
        assert!(!crate::exists(&filepath).await?);

        Ok(())
    }
}
