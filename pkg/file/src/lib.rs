#![feature(generic_arg_infer)]
#![no_std]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate std;

#[macro_use]
extern crate common;

#[macro_use]
extern crate macros;

pub mod allocate_soft;
pub mod dir_lock;
mod error;
mod local;
mod project_path;
mod stdio;
pub mod sync;
pub mod temp;
mod utils;

pub use error::*;
pub use local::*;
pub use project_path::*;
pub use stdio::*;
pub use utils::*;

#[cfg(test)]
mod tests {
    use crate::temp::TempDir;

    use super::*;

    use alloc::borrow::ToOwned;

    use common::errors::*;
    use common::io::{Readable, Writeable};

    #[testcase]
    async fn partially_read_an_existing_file() -> Result<()> {
        let mut file = LocalFile::open(project_path!("testdata/lorem_ipsum.txt"))?;

        let mut buf = vec![0u8; 16];

        file.read_exact(&mut buf).await?;
        assert_eq!(&buf[..], b"Lorem ipsum dolo");

        file.read_exact(&mut buf).await?;
        assert_eq!(&buf[..], b"r sit amet, cons");

        file.read_exact(&mut buf).await?;
        assert_eq!(&buf[..], b"ectetur adipisci");

        file.read_exact(&mut buf).await?;
        assert_eq!(&buf[..], b"ng elit. Duis no");

        file.seek(24);

        file.read_exact(&mut buf).await?;
        assert_eq!(&buf[..], b"et, consectetur ");

        Ok(())
    }

    #[testcase]
    async fn recursively_list_dir_test() -> Result<()> {
        let mut paths = vec![];

        let dir = project_path!("testdata/file/deep_dir");

        recursively_list_dir(&dir, &mut |p| {
            paths.push(p.strip_prefix(&dir).unwrap().to_owned());
        })?;

        paths.sort();

        assert_eq!(
            &paths[..],
            &[
                "cakes/chocolate",
                "cakes/festive/birthday",
                "cakes/festive/fruitcake",
                "carrots",
                "fruit/apples",
                "fruit/blueberries",
            ]
        );

        Ok(())
    }

    #[testcase]
    async fn write_to_a_file() -> Result<()> {
        let temp_dir = TempDir::create()?;

        let path = temp_dir.path().join("my_file");

        // Create a new file with incremental writes.
        {
            let mut file = LocalFile::open_with_options(
                &path,
                &LocalFileOpenOptions::new().create(true).write(true),
            )?;

            file.write_all(b"hello").await?;
            file.write_all(b" world").await?;

            assert_eq!(crate::read_to_string(&path).await?, "hello world");
        }

        // Re-open and append to it.
        {
            let mut file = LocalFile::open_with_options(
                &path,
                &LocalFileOpenOptions::new()
                    .create(true)
                    .append(true)
                    .write(true),
            )?;

            let mut data = vec![];
            data.extend_from_slice(b"! this is cool!");

            file.write_all(&data).await?;

            assert_eq!(
                crate::read_to_string(&path).await?,
                "hello world! this is cool!"
            );
        }

        // Overwrite entire contents.
        {
            crate::write(&path, b"apples").await?;

            assert_eq!(crate::read_to_string(&path).await?, "apples");
        }

        Ok(())
    }

    #[testcase]
    async fn file_existence() -> Result<()> {
        assert_eq!(crate::exists("/nonexistent").await?, false);
        assert_eq!(
            crate::exists(project_path!("testdata/lorem_ipsum.txt")).await?,
            true
        );

        // Files can't exist inside of files (can only exist inside directories).
        assert_eq!(
            crate::exists(project_path!("testdata/lorem_ipsum.txt/hello")).await?,
            false
        );

        Ok(())
    }

    #[testcase]
    async fn file_metadata() -> Result<()> {
        let meta = crate::metadata(&project_path!("testdata/lorem_ipsum.txt")).await?;
        assert_eq!(meta.len(), 3703);
        assert!(meta.is_file());
        assert!(!meta.is_dir());

        let meta = crate::metadata(&project_path!("testdata")).await?;
        assert!(!meta.is_file());
        assert!(meta.is_dir());

        let file = LocalFile::open(project_path!("testdata/lorem_ipsum.txt"))?;
        let meta = file.metadata().await?;
        assert_eq!(meta.len(), 3703);
        assert!(meta.is_file());
        assert!(!meta.is_dir());

        Ok(())
    }
}
