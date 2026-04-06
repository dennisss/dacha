#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;
#[macro_use]
extern crate file;


use std::sync::Arc;

use common::ceil_div;
use common::errors::*;
use graphics::{
    canvas::{Canvas, CanvasHelperExt, Paint},
    font::{CanvasFontRenderer, FontStyle, OpenTypeFont, TextAlign, VerticalAlign},
    image_show::ImageShow,
    raster::canvas::RasterCanvas,
};
use image::{BinaryImage, Color, Image};



#[executor_main]
async fn main() -> Result<()> {

    let code = nordic_bitmaps_generator::generate_font_bitmaps_code().await?;

    println!("{:?}", code);


    Ok(())

}