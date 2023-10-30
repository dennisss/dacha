#[macro_use]
extern crate common;
extern crate image;
#[macro_use]
extern crate file;

use std::fs;

use common::errors::*;
use image::format::bitmap::Bitmap;
use image::format::jpeg::encoder::JPEGEncoder;
use image::format::jpeg::JPEG;
use image::format::qoi::{QOIDecoder, QOIEncoder};
use image::{Color, Image};

use image::format::jpeg::color::jpeg_ycbcr_to_rgb;

fn main() -> Result<()> {
    // 800x600-NV12

    let data = fs::read("image.bin")?;

    let width = 800;
    let height = 600;

    let mut image = Image::<u8>::zero(height, width, image::Colorspace::RGB);

    for y in 0..height {
        for x in 0..width {
            let i = (y * x + x) * 3;

            let r = data[i];
            let g = data[i + 1];
            let b = data[i + 2];

            image.set(y, x, &Color::rgb(r, g, b));
        }
    }

    /*
    let y_plane = &data[0..(width * height)]; // 8 bits for Y
    let uv_plane = &data[(width * height)..];


    for y in 0..height {
        for x in 0..width {
            let o = y * width + height;

            let y_color = y_plane[o];

            let uv_off = (y / 2) * width + ((x / 2) * 2);

            let u_color = uv_plane[uv_off];
            let v_color = uv_plane[uv_off + 1];

            let mut yuv = [y_color, u_color, v_color];
            jpeg_ycbcr_to_rgb(&mut yuv);

            image.set(y, x, &Color::rgb(yuv[0], yuv[1], yuv[2]));
        }
    }
    */

    let encoder = JPEGEncoder::new(80);
    let mut data = vec![];
    encoder.encode(&image, &mut data)?;
    fs::write(project_path!("pi.jpeg"), &data)?;

    return Ok(());

    /*
    let data = fs::read(project_path!("testdata/image/nyhavn.qoi"))?;

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

    let bmp = Bitmap::open(project_path!("testdata/image/valve.bmp"))?;

    bmp.image.show()?;

    let resized = bmp.image.resize(400, 400);

    resized.show()?;
    */

    Ok(())
}
