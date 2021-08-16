#[macro_use]
extern crate common;
extern crate image;

use common::errors::*;
use image::format::bitmap::Bitmap;
use image::format::jpeg::JPEG;

fn main() -> Result<()> {
    let jpg = JPEG::open(project_path!("testdata/jpeg422jfif.jpg")).unwrap();

    // println!("{}", std::mem::size_of::<Vec<u8>>());

    // jpg.image.show()?;

    return Ok(());

    let bmp = Bitmap::open(project_path!("testdata/valve.bmp"))?;

    bmp.image.show()?;

    let resized = bmp.image.resize(400, 400);

    resized.show()?;

    Ok(())
}
