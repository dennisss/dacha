use std::f32::consts::PI;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::ceil_div;
use common::typenum::Pow;
use common::{bytes::Bytes, io::Readable};
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

use crate::ops::layer_image_encoder::*;
use crate::ops::program_visualizer::*;
use crate::program::{ChunkedFileReader, ProgramParserOp, ProgressSender};

pub struct ProgramPreview {
    pub proto: ProgramPreviewProto,
    pub layers_image: Vec<Vec<u8>>,
    pub layer_jpegs: Vec<Bytes>,
}

impl ProgramPreview {
    /*
    Where to run the preview:

    -

    Generating a preview:
    - Input is the file, machine config, and program summary

    - Run ProgramParser
    - Go through the PlayerPreprocessor
        - This is just to ensure that all of the lines can be computed without failure.
    - Go through the ProgramPreview op
        - Goal is to generate per-coordinate system, per-layer, per-tool images

    - Also simulate a precise ETA.
    - Can also compute layer hint points for when to take camera pictures
        - For tool change or multi-part builds, it is best to take pictures
    */
    pub async fn create(
        file_path: &LocalPath,
        machine_config: &MachineConfig,
        summary: &ProgramSummaryProto,
        progress_sender: Option<ProgressSender>,
        generate_jpegs: bool,
    ) -> Result<ProgramPreview> {
        let mut bundle = TaskResultBundle::new();

        let (reader, chunks) = ChunkedFileReader::create(file_path).await?;
        bundle.add("ChunkedFileReader", reader.run());

        let (mut parser, lines) = ProgramParserOp::new(chunks);
        if let Some(sender) = progress_sender {
            let file_size = file::metadata(file_path).await?.len();
            parser.set_progress_reporter(file_size, sender);
        }

        bundle.add("ProgramParser", parser.run());

        // TODO: Run the lines through PlayerPreprocessor as well.

        let (visualizer, visual, images) =
            ProgramVisualizer::create(machine_config, summary, lines)?;
        bundle.add("ProgramVisualizer", visualizer.run());

        let (image_encoder, encoded_images) = LayerImageEncoder::new(images, generate_jpegs);
        bundle.add("LayerImageEncoder", image_encoder.run());

        bundle.join().await?;

        let proto = visual
            .recv()
            .await
            .map_err(|_| err_msg("No summary for generated for an unknown reason"))?;

        let images = encoded_images
            .recv()
            .await
            .map_err(|_| err_msg("No layer images generated for some reason"))?;

        Ok(Self {
            proto,
            layers_image: images.binary_images,
            layer_jpegs: images.jpegs,
        })
    }

    //
}
