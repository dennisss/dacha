#[macro_use]
extern crate common;
extern crate dymo;
extern crate graphics;
extern crate image;

use common::bits::BitVector;
use common::errors::*;
use graphics::font::CanvasFontExt;
use graphics::font::OpenTypeFont;
use graphics::image_show::ImageShow;
use graphics::raster::canvas::Canvas;

/*
NOTE: Max packet size is 64


Typical response status:

[40, 00, 12, 64, 0d, 85, 00, 00]
                      ^ Probably the battery percentage?
  ^ When it is 60, it seems to still be printing

*/

/*

9mm x 2.8mm is the 64 x 20

~0.14mm square per pixel

aka ~180 DPI


Status:
'0x1b A'

Output: 8

*/

async fn run() -> Result<()> {
    let mut canvas = Canvas::create(64, 320);

    let font = OpenTypeFont::open(project_path!("testdata/noto-sans.ttf")).await?;

    canvas.drawing_buffer.clear_white();

    let color = image::Color::zero(3);
    canvas.fill_text(50.0, 57.0, &font, "4B 4G", 50.0, &color)?;

    canvas.drawing_buffer.show().await?;

    let mut lines = vec![];
    for x in 0..canvas.drawing_buffer.width() {
        let mut line = BitVector::new();
        for y in (0..canvas.drawing_buffer.height()).rev() {
            let c = canvas.drawing_buffer.get(y, x);
            // TODO: Add a helper to check if an entire matrix is exactly zero.

            let bit = {
                if c[0] == 0 {
                    1
                } else {
                    0
                }
            };

            line.push(bit);
        }

        lines.push(line.as_ref().to_vec());
    }

    let manager = dymo::LabelManager::open().await?;

    manager.print_label(&lines).await?;

    /*
    let mut lines = vec![];

    for j in 0..3 {
        for i in 0..10 {
            let mut line = vec![];
            line.resize(8, 0xff);
            lines.push(line);
        }

        for i in 0..10 {
            let mut line = vec![];
            line.resize(8, 0);
            lines.push(line);
        }
    }

    for i in 0..100 {
        let mut line = vec![];
        line.resize(8, 0);
        lines.push(line);
    }


    */

    loop {
        let status = manager.read_status().await?;

        println!("{:?}", status);

        common::wait_for(std::time::Duration::from_secs(1)).await;
    }

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
