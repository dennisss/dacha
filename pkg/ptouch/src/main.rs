#[macro_use]
extern crate common;
extern crate ptouch;

use common::errors::*;
use graphics::{
    canvas::{Canvas, CanvasHelperExt, Paint},
    font::{CanvasFontRenderer, FontStyle, OpenTypeFont, TextAlign, VerticalAlign},
    image_show::ImageShow,
    raster::canvas::RasterCanvas,
};
use image::{Color, Image};
use ptouch::*;

async fn run() -> Result<()> {
    let text = "Hello";
    let font_size = 100.0;

    let font = OpenTypeFont::read(project_path!("third_party/noto_sans/font_normal.ttf")).await?;
    let font_renderer = CanvasFontRenderer::new(font);

    let measurements = font_renderer.measure_text(text, font_size, None)?;

    let height = 128.0;

    let mut canvas = RasterCanvas::create(height as usize, (measurements.width + 1.) as usize);
    let c = &mut canvas as &mut dyn Canvas;
    c.clear_rect(
        0.,
        0.,
        measurements.width,
        height,
        &Color::rgb(255, 255, 255),
    )?;

    let font_style = FontStyle::from_size(font_size)
        .with_text_align(TextAlign::Left)
        .with_vertical_align(VerticalAlign::Center);

    let paint = Paint::color(Color::hex(0));

    font_renderer.fill_text(0.0, height / 2.0, text, &font_style, &paint, &mut canvas)?;

    canvas.drawing_buffer.show().await?;

    let mut dev = LabelMaker::open().await?;

    dev.get_info().await?;

    // dev.get_status().await?;

    /*
    let mut image = Image::<u8>::zero(128, 100, image::Colorspace::RGB);

    image.clear_white();

    for i in 0..100 {
        image.set(64, i, &Color::zero())
    }
    */
    let image = &canvas.drawing_buffer;

    dev.print(image).await?;

    // for i in 0..2 {
    //     dev.print(&image).await?;
    // }

    Ok(())
}

fn main() -> Result<()> {
    executor::run(run())?
}
