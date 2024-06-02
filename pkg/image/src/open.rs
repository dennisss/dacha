use common::errors::*;
use file::LocalPath;

use crate::format::jpeg::JPEG;
use crate::format::qoi::QOIDecoder;
use crate::types::ImageType;
use crate::Image;

impl Image<u8> {
    pub async fn read<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // TODO: Re-implement usage of this.
        let ext = path.extension().map(|ext| ext.to_ascii_lowercase());

        let data = file::read(path).await?;

        Self::parse_from(&data)

        /*
        match ext.as_ref().map(|s| s.as_ref()) {
            // TODO: Switch this to use async_std.
            Some("jpeg") | Some("jpg") => Ok(JPEG::open(path)?.image),
            Some("qoi") => {
                let data = file::read(path).await?;
                QOIDecoder::new().decode(&data)
            }
            _ => Err(err_msg("Unknown image format")),
        }
        */
    }

    pub fn parse_from(data: &[u8]) -> Result<Self> {
        let typ = ImageType::from_header(data)
            .ok_or_else(|| err_msg("Can't determing the type of the image"))?;

        Ok(match typ {
            ImageType::JPEG => JPEG::parse(data)?.image,
            ImageType::PNG => todo!(),
            ImageType::QOI => QOIDecoder::new().decode(&data)?,
            ImageType::BMP => todo!(),
        })
    }
}
