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
mod local;
mod project_path;
mod stdio;
pub mod sync;
pub mod temp;
mod utils;

pub use local::*;
pub use project_path::*;
pub use stdio::*;
pub use utils::*;

#[cfg(test)]
mod tests {
    use super::*;

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
}
