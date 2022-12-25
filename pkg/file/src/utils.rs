use alloc::vec::Vec;

use common::errors::*;
use common::io::Readable;

use crate::{LocalFile, LocalPath};

pub async fn read<P: AsRef<LocalPath>>(path: P) -> Result<Vec<u8>> {
    let mut out = vec![];
    let mut file = LocalFile::open(path).await?;
    file.read_to_end(&mut out).await?;
    Ok(out)
}
