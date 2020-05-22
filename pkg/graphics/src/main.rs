extern crate common;
extern crate graphics;
extern crate math;

use common::errors::*;
use graphics::app::*;
use graphics::polygon::Polygon;
use graphics::shader::Shader;
use math::matrix::{Vector2i, Vector3f};
use std::sync::Arc;

async fn run() -> Result<()> {
    let mut app = Application::new();
    let window = app.create_window("My Window", &Vector2i::from_slice(&[300, 300]), true);

    let shader = Arc::new(Shader::Default().await?);
    let poly = Polygon::regular_mono(3, &Vector3f::from_slice(&[1.0, 0.0, 0.0]), shader.clone());

    //	let poly = Polygon::from(&[
    //		Vector3f::from_slice(&[0.0, 0.5, 0.0]),
    //		Vector3f::from_slice(&[0.5, -0.5, 0.0]),
    //		Vector3f::from_slice(&[-0.5, -0.5, 0.0]),
    //	], &[
    //		Vector3f::from_slice(&[0.0, 1.0, 1.0]),
    //		Vector3f::from_slice(&[1.0, 0.0, 1.0]),
    //		Vector3f::from_slice(&[1.0, 1.0, 0.0]),
    //	], shader.clone());

    window.lock().unwrap().scene.add_object(Box::new(poly));

    app.run();

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(graphics::font::open_font())

    //    graphics::raster::run()

    //	async_std::task::block_on(run())

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
