#[macro_use]
extern crate common;
extern crate graphics;
extern crate image;
extern crate math;

use std::sync::Arc;

use common::errors::*;
use graphics::image_show::ImageShow;
use graphics::transform::orthogonal_projection;
use image::format::qoi::QOIDecoder;
use math::matrix::{Vector2f, Vector2i, Vector3f};

/*
Application:
- Maintains a render thread.
- Windows are only identified by an id and a shared pointer to all of the state for that window.

*/

async fn run() -> Result<()> {
    let image_data = common::async_std::fs::read(project_path!("testdata/nyhavn.qoi")).await?;
    let mut image = QOIDecoder::new().decode(&image_data)?;

    image.show().await?;

    Ok(())
}

fn main() -> Result<()> {
    // let f = run();
    // let f = graphics::font::open_font();
    let f = graphics::ui::examples::run();
    // let f = graphics::point_picker::run();
    // let f = graphics::opengl::run();

    return common::async_std::task::block_on(f);

    // common::async_std::task::block_on(graphics::font::open_font())

    // let task = graphics::font::open_font();

    // let task = graphics::raster::run();

    // common::async_std::task::block_on(task)

    /*
        Default opengl mode:
        - -1 to 1 in all dimensions
        - Step 1: normalize to 0 to width and 0 to height (top-left corner is (0,0))
        - Step 2: Assume z is 0 for now (we will keep around z functionality to
          enable easy switching to 3d)
        -

        TODO: Premultiply proj by modelview for each object?
    */
}
