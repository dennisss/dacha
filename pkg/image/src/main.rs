#[macro_use]
extern crate common;
extern crate image;

use std::fs;

use common::errors::*;
use image::format::bitmap::Bitmap;
use image::format::jpeg::encoder::JPEGEncoder;
use image::format::jpeg::JPEG;
use image::format::qoi::{QOIDecoder, QOIEncoder};

fn main() -> Result<()> {
    let data = file::read(project_path!("testdata/nyhavn.qoi"))?;

    let mut image = QOIDecoder::new().decode(&data)?;

    let mut encoded = vec![];
    QOIEncoder::new().encode(&image, &mut encoded)?;
    fs::write(project_path!("encoded.qoi"), &data)?;

    image = image.resize(image.height() / 2, image.width() / 2);
    image.show()?;

    return Ok(());

    // let jpg1 = JPEG::open(project_path!("data/picsum/15.jpeg"))?;
    // let jpg2 = JPEG::open(project_path!("encoded.jpeg"))?;

    // for i in 0..100 {
    //     let diff = (jpg1.image.array.data[i] as i32) - (jpg2.image.array.data[i]
    // as i32);     println!("{}: {}", i, diff);
    // }

    // return Ok(());

    // let jpg = JPEG::open(project_path!("encoded.jpeg"))?;
    let jpg = JPEG::open(project_path!("data/picsum/15.jpeg"))?;
    // let jpg = JPEG::open(project_path!("pic.jpeg"))?;
    // let jpg = JPEG::open(project_path!("ext/jpeg422jfif.jpg")).unwrap();

    let encoder = JPEGEncoder::new(80);
    let mut data = vec![];
    encoder.encode(&jpg.image, &mut data)?;
    fs::write(project_path!("encoded.jpeg"), &data)?;

    jpg.image.show()?;

    // println!("{}", std::mem::size_of::<Vec<u8>>());

    // jpg.image.show()?;

    return Ok(());

    let bmp = Bitmap::open(project_path!("testdata/valve.bmp"))?;

    bmp.image.show()?;

    let resized = bmp.image.resize(400, 400);

    resized.show()?;

    Ok(())
}
