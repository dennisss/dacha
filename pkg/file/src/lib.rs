#![feature(generic_arg_infer, let_chains)]
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

    #[testcase]
    async fn rename_test() -> Result<()> {
        let temp_dir = TempDir::create()?;

        let first_path = temp_dir.path().join("first");
        let second_path = temp_dir.path().join("second");

        crate::write(&first_path, "hello").await?;
        assert!(crate::exists(&first_path).await?);
        assert!(!crate::exists(&second_path).await?);

        crate::rename(&first_path, &second_path).await?;

        assert!(!crate::exists(&first_path).await?);
        assert!(crate::exists(&second_path).await?);
        assert_eq!(crate::read_to_string(&second_path).await?, "hello");

        Ok(())
    }

    #[testcase]
    async fn link_creation_test() -> Result<()> {
        let temp_dir = TempDir::create()?;

        {
            let link_path = temp_dir.path().join("my_link");
            crate::symlink(&project_path!("testdata/lorem_ipsum.txt"), &link_path).await?;

            let meta = crate::metadata(&link_path).await?;
            assert!(meta.is_file());
            assert_eq!(meta.len(), 3703);

            let meta = crate::symlink_metadata(&link_path).await?;
            assert!(meta.is_symlink());
        }

        // Testing that relative paths are resolved relative to the link location.
        {
            let link2_path = temp_dir.path().join("my_link2");
            let file_path = temp_dir.path().join("hello_file");

            crate::symlink(LocalPath::new("hello_file"), &link2_path).await?;

            crate::write(&file_path, b"hi there this is data").await?;
            assert_eq!(
                crate::read_to_string(&link2_path).await?,
                "hi there this is data"
            );

            crate::write(&link2_path, b"new").await?;
            assert_eq!(crate::read_to_string(&file_path).await?, "new");

            let meta = crate::metadata(&link2_path).await?;
            assert!(meta.is_file());
            assert_eq!(meta.len(), 3);

            let meta = crate::symlink_metadata(&link2_path).await?;
            assert!(meta.is_symlink());
            assert_eq!(meta.len(), 10); // Length of the path "hello_file"
        }

        Ok(())
    }

    #[testcase]
    async fn create_dir_all_all() -> Result<()> {
        let temp_dir = TempDir::create()?;

        let dir = temp_dir.path().join("a/b/c");
        crate::create_dir_all(&dir).await?;
        crate::create_dir_all(&dir).await?;

        assert!(crate::exists(temp_dir.path().join("a")).await?);
        assert!(crate::exists(temp_dir.path().join("a/b")).await?);
        assert!(crate::exists(temp_dir.path().join("a/b/c")).await?);

        crate::write(temp_dir.path().join("a/b/c/d"), b"hello").await?;

        Ok(())
    }

    #[testcase]
    async fn remove_dir_all() -> Result<()> {
        let temp_dir = TempDir::create()?;

        crate::create_dir_all(temp_dir.path().join("a/b/c")).await?;
        crate::create_dir_all(temp_dir.path().join("a/e")).await?;
        crate::write(temp_dir.path().join("a/f"), b"data").await;

        crate::create_dir_all(temp_dir.path().join("other")).await?;
        crate::write(temp_dir.path().join("other/data"), b"yo!").await;
        crate::create_dir_all(temp_dir.path().join("empty")).await?;
        crate::write(temp_dir.path().join("file"), b"hi!").await;

        crate::symlink(
            temp_dir.path().join("other"),
            temp_dir.path().join("a/b/c/other"),
        )
        .await?;
        crate::symlink(
            temp_dir.path().join("empty"),
            temp_dir.path().join("a/b/bats"),
        )
        .await?;
        crate::symlink(temp_dir.path().join("file"), temp_dir.path().join("a/cats")).await?;

        crate::remove_dir_all(&temp_dir.path().join("a"))
            .await
            .unwrap();

        assert!(!crate::exists(temp_dir.path().join("a")).await?);
        assert!(crate::exists(temp_dir.path().join("empty")).await?);
        assert!(crate::exists(temp_dir.path().join("file")).await?);
        assert!(crate::exists(temp_dir.path().join("other/data")).await?);

        Ok((()))
    }

    #[testcase]
    async fn copy_test() -> Result<()> {
        let temp_dir = TempDir::create()?;

        let from = temp_dir.path().join("a");
        let to = temp_dir.path().join("b");

        // TODO: Test for very large files which may require multiple separate writes.
        crate::write(&from, b"hi").await?;
        crate::copy(&from, &to).await?;
        assert_eq!(crate::read_to_string(&to).await?, "hi");

        Ok(())
    }

    #[testcase]
    async fn copy_all_test() -> Result<()> {
        let temp_dir = TempDir::create()?;

        let from = temp_dir.path().join("a");
        let to = temp_dir.path().join("b");

        crate::create_dir_all(from.join("b")).await?;
        crate::write(from.join("b/c"), b"hi").await?;

        crate::copy_all(&from, &to).await.unwrap();

        assert_eq!(crate::read_to_string(&to.join("b/c")).await?, "hi");

        Ok(())
    }

    #[testcase]
    async fn opening_at_an_empty_path_fails() -> Result<()> {
        let res = LocalFile::open("");
        assert_eq!(
            res.err().unwrap().downcast_ref::<FileError>(),
            Some(&FileError::NotFound)
        );

        // std::fs::File also should fail.
        std::fs::File::open("").unwrap_err();

        Ok(())
    }
}
