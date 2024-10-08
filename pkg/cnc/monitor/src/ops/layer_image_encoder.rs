use std::f32::consts::PI;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::async_std::task::current;
use common::ceil_div;
use common::typenum::Pow;
use common::{bytes::Bytes, io::Readable};
use compression::transform::partially_transform_to_vec;
use executor::{
    bundle::TaskResultBundle,
    channel::{self, oneshot, spsc},
    sync::SyncMutex,
};
use file::{LocalFile, LocalPath};
use gcode::CommandCodec;
use graphics::canvas::{Canvas, Paint, PathBuilder};
use graphics::transforms::transform2f;
use image::Color;
use image::{format::jpeg::encoder::JPEGEncoder, types::ImageType, Image};
use math::matrix::cwise_binary_ops::{CwiseMax, CwiseMin};
use math::matrix::{Vector2f, Vector3f};

pub struct LayerImageEncoder {
    input: spsc::Receiver<Image<u8>>,
    output: oneshot::Sender<LayerImages>,
    generate_jpegs: bool,
}

pub struct LayerImages {
    pub binary_images: Vec<Vec<u8>>,
    pub jpegs: Vec<Bytes>,
}

impl LayerImageEncoder {
    pub fn new(
        input: spsc::Receiver<Image<u8>>,
        generate_jpegs: bool,
    ) -> (Self, oneshot::Receiver<LayerImages>) {
        let (sender, receiver) = oneshot::channel();

        let inst = Self {
            input,
            output: sender,
            generate_jpegs,
        };

        (inst, receiver)
    }

    pub async fn run(mut self) -> Result<()> {
        let mut binary_images = vec![];
        let mut jpegs = vec![];

        // TODO: The main reason this is very slow right now is that we don't support
        // using the input buffer as the sliding window for match lookups (instead we
        // always copy the data into a cyclic buffer).
        let mut compressor = compression::zlib::ZlibEncoder::new();

        while let Ok(image) = self.input.recv().await {
            let binary_image_raw = encode_binary_image(&image)?;

            let mut out = vec![];
            partially_transform_to_vec(&mut compressor, &binary_image_raw, false, &mut out)?;
            binary_images.push(out);

            if self.generate_jpegs {
                let jpeg = {
                    let mut encoded = vec![];
                    let encoder = JPEGEncoder::new(90);
                    encoder.encode(&image, &mut encoded)?;
                    encoded
                };

                jpegs.push(jpeg.into());
            }
        }

        let mut final_out = vec![];
        partially_transform_to_vec(&mut compressor, &[], true, &mut final_out)?;
        binary_images.push(final_out);

        let _ = self.output.send(LayerImages {
            binary_images: binary_images.into(),
            jpegs,
        });

        Ok(())

        //
    }
}

#[inline(never)]
fn encode_binary_image(image: &Image<u8>) -> Result<Vec<u8>> {
    if image.channels() != 1 {
        return Err(err_msg("Expected a one channel image to encode"));
    }

    let mut out = vec![];

    out.extend_from_slice(b"daBI"); // Magic
    out.extend_from_slice(&[1, 0, 0, 0]); // Version/flags
    out.extend_from_slice(&(image.height() as u32).to_le_bytes());
    out.extend_from_slice(&(image.width() as u32).to_le_bytes());

    let height = image.height();
    let width = image.width();

    out.reserve_exact(height * ceil_div(width, 8));

    let mut x = 0;
    let mut cur = 0;
    for v in image.array.data.iter().cloned() {
        if x % 8 == 0 {
            out.push(cur);
            cur = 0;
        }

        if v != 0 {
            cur |= 1 << (7 - (x % 8));
        }

        x += 1;
        if x == width {
            x = 0;
        }
    }

    if x != 0 {
        out.push(cur);
    }

    Ok(out)
}
