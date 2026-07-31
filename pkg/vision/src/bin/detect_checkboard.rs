/*
cargo run --bin detect_checkboard --release
*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;
use image::format::jpeg::encoder::JPEGEncoder;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::bundle::TaskResultBundle;
use executor::FileHandle;
use file::LocalPathBuf;
use macros::executor_main;
use math::array::Array;
use image::{Colorspace, Image};
use file::project_path;
use vision::*;
use math::matrix::{vec2d, vec3d, Matrix3d, Vector3d, Vector2d};
use math::matrix::axis_angle::to_axis_angle;


#[executor_main]
async fn main() -> Result<()> {
    let input_path = project_path!(
        // "data/mocap_camera_calib/r7hdsr8h9fyhe/0018_0000.jpg"
    
        // "data/mocap_camera_calib/ab21z2zt1gf6w/0003_0000_bad.jpg"

        // "data/mocap_camera_calib/ab21z2zt1gf6w/0006_0000.jpg"

        // "/home/dennis/workspace/dacha/data/mocap_camera_calib/ab21z2zt1gf6w/0004_0000.jpg"

        "data/mocap_data_dir/checkerboard/jg30xx5m7wcky/1784779613163867/0000.jpg"
    );

    let output_dir = project_path!("data/checkerboard_debug");

    let img = Image::<u8>::read(&input_path).await?;

    let mut options = CheckerboardDetectionOptions::default();
    options.grid_width = 8;
    options.grid_height = 13;

    /*
    8 x 13 (40mm spacing)
    */
    let mut res = detect_checkboard(&img, &options).await;

    for (i, img) in res.debug_images.into_iter().enumerate() {

        let p = output_dir.join(format!("{:04}.jpg", i));

        let encoder = JPEGEncoder::new(100);
        let mut data = vec![];
        encoder.encode(&img, &mut data)?;

        file::write(p, &data).await?;
    }


    Ok(())
}