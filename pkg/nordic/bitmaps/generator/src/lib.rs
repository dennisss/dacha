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


pub async fn generate_font_bitmaps_code() -> Result<String> {
    let font = Arc::new(
        OpenTypeFont::read(file::project_path!("third_party/noto_sans/font_mono_normal.ttf"))
            .await?,
    );

    let font_renderer = CanvasFontRenderer::new(font.clone());

    let font_size = 40.0;

    let raw_size = font_renderer.measure_text("A", font_size, None)?;
    
    let bitmap_width_bytes = (raw_size.width / 8.0).ceil() as usize;
    let bitmap_width = bitmap_width_bytes * 8;
    let bitmap_height = raw_size.height.ceil() as usize;

    println!("Bitmap Size: {} x {}", bitmap_height, bitmap_width);

    let mut canvas = RasterCanvas::create_grayscale(bitmap_height, bitmap_width);

    let font_style = FontStyle::from_size(font_size)
        .with_text_align(TextAlign::Left)
        .with_vertical_align(VerticalAlign::Top);

    let paint = Paint::color(Color::hex(0));

    let mut out = String::new();
    let mut list_values = vec![];

    for char_code in 0u32..256 {
        let char_value = char::from_u32(char_code).unwrap();
        
        let valid = char_value.is_ascii_alphanumeric() || char_value.is_ascii_punctuation();

        if !valid {
            list_values.push("None".to_string());
            continue;
        }

        println!("Generate bitmap for '{}'", char_value);

        let const_name = format!("FONT_BITMAP_{}", char_code);
        
        {
            let c = &mut canvas as &mut dyn Canvas;

            c.clear_rect(
                0.,
                0.,
                bitmap_width as f32,
                bitmap_height as f32,
                &Color::rgb(255, 255, 255),
            )?;

            font_renderer.fill_text(
                0.0,
                0.0,
                &format!("{}", char_value),
                &font_style,
                &paint,
                c,
            )?;
        }

        {
            let mut image_data = &canvas.drawing_buffer.array.data;
            let mut bitmap = vec![0u8; bitmap_width_bytes * bitmap_height];

            for i in 0..image_data.len() {
                let bit = if image_data[i] != 0 { 1 } else { 0 };

                let offset = i / 8;
                let bit_offset = 7 - (i % 8);

                bitmap[offset] |= bit << bit_offset;
            }


            out.push_str(&format!(
                "const {name}: BitmapImageRef = BitmapImageRef {{ width: {width}, height: {height}, data: &{data:?} }};\n",
                name = const_name,
                width = bitmap_width,
                height = bitmap_height,
                data = bitmap
            ));

            list_values.push(format!("Some(&{})", const_name));
        }

    } 

    out.push_str(&format!(
        "const FONT_BITMAP_LIST: [Option<&'static BitmapImageRef<'static>>; 256] = [{}];\n",
        list_values.join(", ")
    ));

    Ok(out)
}