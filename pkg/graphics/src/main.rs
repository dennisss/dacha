#[macro_use]
extern crate common;
extern crate graphics;
extern crate image;
extern crate math;
#[macro_use]
extern crate file;
#[macro_use]
extern crate macros;

use std::{f32::consts::PI, os::raw::c_void, sync::Arc};

use common::errors::*;
use gl::types::{GLint, GLsizei, GLuint};
use graphics::image_show::ImageShow;
use graphics::transform::orthogonal_projection;
use image::format::qoi::QOIDecoder;
use math::matrix::{Dimension, StaticDim, Vector, Vector2f, Vector2i, Vector3f};

/*
Application:
- Maintains a render thread.
- Windows are only identified by an id and a shared pointer to all of the state for that window.

*/

fn gaussian_pdf(x: f32, sigma: f32, mean: f32) -> f32 {
    ((x - mean).powi(2) / (-2.0 * sigma * sigma)).exp() / (sigma * (2.0 * PI).sqrt())
}

fn gaussian_1d_filter<D: StaticDim>(sigma: f32) -> Vector<f32, D> {
    let mut v = Vector::zero();
    let mid = (D::to_usize() / 2) as f32;
    for i in 0..v.len() {
        v[i] = gaussian_pdf(i as f32, sigma, mid);
    }

    v
}

async fn run() -> Result<()> {
    let g = gaussian_1d_filter::<typenum::U5>(2.0);
    let mut g2d = &g * g.transpose();

    // Normalize filter
    {
        let mut s = 0.0;
        for i in 0..g2d.len() {
            s += g2d[i].abs();
        }
        // s /= g2d.len() as f32;

        g2d /= s;
    }

    println!("{:?}", g2d);

    let image_data = file::read(project_path!("testdata/image/nyhavn.qoi")).await?;
    let mut image = QOIDecoder::new().decode(&image_data)?;

    let mut app = graphics::opengl::app::Application::new();
    let mut window = app.create_window("Compute", Vector2i::from_slice(&[10, 10]), false, false);

    let shader_src =
        file::read_to_string(project_path!("pkg/graphics/shaders/canny.compute.glsl")).await?;

    let shader = graphics::opengl::shader::Shader::load_compute(&shader_src, &mut window)?;

    window.context().make_current();

    println!("{:?}", image.colorspace);

    // Make a texture for the input image.
    let mut input_texture = 0;
    unsafe {
        gl::GenTextures(1, &mut input_texture);
        gl::ActiveTexture(gl::TEXTURE0);
        gl::BindTexture(gl::TEXTURE_2D, input_texture);

        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as GLint);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as GLint);

        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);

        gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

        // TODO: Support different pixel formats
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as GLint,
            image.width() as GLsizei,
            image.height() as GLsizei,
            0,
            gl::RGB,
            gl::UNSIGNED_BYTE,
            core::mem::transmute(image.array.data.as_ptr()),
        );

        // Reset override.
        gl::PixelStorei(gl::UNPACK_ALIGNMENT, 4);

        gl::BindImageTexture(0, input_texture, 0, gl::FALSE, 0, gl::READ_ONLY, gl::RGBA8);
    }

    // Make empty output texture
    let mut output_texture = 0;
    unsafe {
        gl::GenTextures(1, &mut output_texture);
        gl::ActiveTexture(gl::TEXTURE1);
        gl::BindTexture(gl::TEXTURE_2D, output_texture);

        gl::TexParameteri(
            gl::TEXTURE_2D,
            gl::TEXTURE_WRAP_S,
            gl::CLAMP_TO_EDGE as GLint,
        );
        gl::TexParameteri(
            gl::TEXTURE_2D,
            gl::TEXTURE_WRAP_T,
            gl::CLAMP_TO_EDGE as GLint,
        );

        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);

        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA8 as GLint,
            image.width() as GLsizei,
            image.height() as GLsizei,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            core::ptr::null(),
        );

        gl::BindImageTexture(
            1,
            output_texture,
            0,
            gl::FALSE,
            0,
            gl::WRITE_ONLY,
            gl::RGBA8,
        );
    }

    // run the compute
    unsafe {
        gl::UseProgram(shader.program);
        gl::DispatchCompute(image.width() as GLuint, image.height() as GLuint, 1);
        gl::MemoryBarrier(gl::ALL_BARRIER_BITS);
    }

    // dump output
    let mut data: Vec<u8> = vec![0; 4 * image.width() * image.height()];
    unsafe {
        gl::GetTextureImage(
            output_texture,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            (data.len() * core::mem::size_of::<u8>()) as i32,
            data.as_ptr() as *mut c_void,
        );
    }

    println!("{:?}", &data[0..50]);

    let mut i = image::Image::<u8>::zero(image.height(), image.width(), image::Colorspace::RGBA);

    for (idx, v) in data.into_iter().enumerate() {
        i.array.data[idx] = v;
    }

    i.show().await?;

    // i.array.data.copy_from_slice(data);

    /*

    // Rendering stuff

    computeShader.use();
    glDispatchCompute((unsigned int)TEXTURE_WIDTH, (unsigned int)TEXTURE_HEIGHT, 1);

    // make sure writing to image has finished before read
    glMemoryBarrier(GL_SHADER_IMAGE_ACCESS_BARRIER_BIT);

    */

    //

    /*


    image.show().await?;
    */

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let f = run();
    // let f = graphics::font::open_font();
    // let f = graphics::ui::examples::run();
    // let f = graphics::point_picker::run();
    // let f = graphics::opengl::run();

    f.await

    // executor::run(graphics::font::open_font())?

    // let task = graphics::font::open_font();

    // let task = graphics::raster::run();

    // executor::run(task)?

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
