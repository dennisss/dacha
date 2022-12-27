use common::errors::*;
use file::LocalPath;

use crate::format::jpeg::JPEG;
use crate::format::qoi::QOIDecoder;
use crate::Image;

impl Image<u8> {
    pub async fn read<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        let ext = path.extension().map(|ext| ext.to_ascii_lowercase());

        match ext.as_ref().map(|s| s.as_ref()) {
            // TODO: Switch this to use async_std.
            Some("jpeg") | Some("jpg") => Ok(JPEG::open(path)?.image),
            Some("qoi") => {
                let data = file::read(path).await?;
                QOIDecoder::new().decode(&data)
            }
            _ => Err(err_msg("Unknown image format")),
        }
    }
}
