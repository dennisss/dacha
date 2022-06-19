use alloc::rc::Rc;
use core::future::Future;

use common::errors::*;
use image::Image;
use math::matrix::{Vector2f, Vector2i, Vector3f};

use crate::opengl::app::Application;
use crate::opengl::polygon::Polygon;
use crate::opengl::shader::*;
use crate::opengl::texture::Texture;
use crate::transform::orthogonal_projection;
use crate::ui::element::Element;
use crate::ui::examples::image_viewer::ImageViewer;
use crate::ui::render::render_element;

const MAX_DIMENSION: f32 = 1000.0;

/// Helper for displaying a single image from a command line program.
pub trait ImageShow {
    type ShowFuture: Future<Output = Result<()>>;

    // TODO: Spawn this on a separate thread so that it doesn't block.
    fn show(&self) -> Self::ShowFuture;
}

impl ImageShow for Image<u8> {
    type ShowFuture = impl Future<Output = Result<()>>;

    fn show(&self) -> Self::ShowFuture {
        let (window_width, window_height) = {
            let aspect_ratio = (self.width() as f32) / (self.height() as f32);

            if self.width() < self.height() {
                (
                    (aspect_ratio * MAX_DIMENSION).round() as usize,
                    MAX_DIMENSION as usize,
                )
            } else {
                (
                    MAX_DIMENSION as usize,
                    (MAX_DIMENSION / aspect_ratio).round() as usize,
                )
            }
        };

        let root_el = Element::from(ImageViewer {
            source: Rc::new(self.clone()),
            outer_height: window_height,
            outer_width: window_width,
        });

        render_element(root_el, window_height, window_width)
    }
}
