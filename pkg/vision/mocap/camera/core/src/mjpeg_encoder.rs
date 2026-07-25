use common::errors::*;
use image::ImageRef;
use image::format::jpeg::encoder::JPEGEncoder;
use mocap_proto::mocap::MJPEGEncoderConfig;

use crate::image_processing::*;


pub struct MJPEGEncoder {
    encoder: JPEGEncoder,
    downsample_buffer: Vec<u8>,
    threshold_buffer: Vec<u8>,
}

impl MJPEGEncoder {
    pub fn default_config() -> Result<MJPEGEncoderConfig> {
        let mut config = MJPEGEncoderConfig::default();
        protobuf::text::parse_text_proto(r#"
            quality: 100
            downsampling: 1
            max_fps: 5
        "#, &mut config)?;
        Ok(config)
    }

    pub fn new() -> Self {
        let mut encoder = JPEGEncoder::new(100);
        encoder.use_default_tables();
        
        Self {
            encoder,
            downsample_buffer: vec![],
            threshold_buffer: vec![]
        }
    }

    pub fn encode<'a>(&'a mut self, mut image: ImageRef<'a>, pixel_threshold: u8, config: &MJPEGEncoderConfig) -> Result<Vec<u8>> {

        if config.thresholded() {
            self.threshold_buffer.resize(image.data.len(), 0);
            apply_threshold(&image.data, &mut self.threshold_buffer, pixel_threshold);
            image.data = &self.threshold_buffer;
        }

        if config.downsampling() == 2 {
            self.downsample_buffer.resize(image.data.len() / 4, 0);
            downscale_2x(&image.data, &mut self.downsample_buffer, image.width, image.height);

            image.width /= 2;
            image.height /= 2;
            image.data = &self.downsample_buffer;
        }

        self.encoder.set_quality(config.quality() as usize);

        let mut data = vec![];
        data.reserve_exact(image.data.len() / 8);

        self.encoder.encode_raw(&image, &mut data)?;

        Ok(data)
    }
}
