use core::any::Any;

use common::errors::*;
use common::iter::PairIter;
use image::{Color, Colorspace, Image};
use math::matrix::{vec2f, Matrix3f, Vector2f, Vector2i, Vector3f};

use crate::canvas::base::CanvasBase;
use crate::canvas::curve::Curve;
use crate::canvas::path::*;
use crate::canvas::*;
use crate::raster::FillRule;
use crate::transforms::transform2f;

/// 2D Canvas implementation which performs all pixel rasterization on CPU.
pub struct RasterCanvas {
    // TODO: Make private.
    pub drawing_buffer: Image<u8>,
    base: CanvasBase,
}

impl_deref!(RasterCanvas::base as CanvasBase);

impl RasterCanvas {
    pub fn create(height: usize, width: usize) -> Self {
        Self {
            drawing_buffer: Image::zero(height, width, Colorspace::RGB),
            base: CanvasBase::new(),
        }
    }
}

impl Canvas for RasterCanvas {
    fn create_path_fill(&mut self, path: &Path) -> Result<Box<dyn CanvasObject>> {
        let (vertices, path_starts) = path.linearize(self.base.current_transform());

        // TODO: Currently this assumes that all subpaths are closed. Possibly we should
        // instead Optimize this more and not use all of the empty line segments
        // between lines.
        Ok(Box::new(RasterCanvasPath {
            vertices,
            path_starts,
        }))
    }

    /// TODO: Must use the non-zero winding rule for this always.
    /// TODO: This has a lot of redundant computation with fill_path if we ever
    /// want to do both.
    fn create_path_stroke(&mut self, path: &Path, width: f32) -> Result<Box<dyn CanvasObject>> {
        let (vertices, path_starts) = path.stroke(width, self.current_transform());

        Ok(Box::new(RasterCanvasPath {
            vertices,
            path_starts,
        }))
    }

    fn create_image(&mut self, image: &Image<u8>) -> Result<Box<dyn CanvasObject>> {
        Ok(Box::new(RasterCanvasImage {
            image: image.clone(),
        }))
    }

    // pub fn clear() {}
}

pub struct RasterCanvasPath {
    vertices: Vec<Vector2f>,
    path_starts: Vec<usize>,
}

impl CanvasObject for RasterCanvasPath {
    fn draw(&mut self, paint: &crate::canvas::Paint, canvas: &mut dyn Canvas) -> Result<()> {
        let canvas = canvas.as_mut_any().downcast_mut::<RasterCanvas>().unwrap();

        // TODO: Implement alpha mixing.

        crate::raster::fill_polygon(
            &mut canvas.drawing_buffer,
            &self.vertices,
            &paint.color,
            &self.path_starts,
            FillRule::NonZero,
        )?;

        Ok(())
    }
}

pub struct RasterCanvasImage {
    image: Image<u8>,
}

impl CanvasObject for RasterCanvasImage {
    fn draw(&mut self, paint: &Paint, canvas: &mut dyn Canvas) -> Result<()> {
        let canvas = canvas.as_mut_any().downcast_mut::<RasterCanvas>().unwrap();

        let white = Color::rgb(255, 255, 255);

        for y in 0..self.image.height() {
            for x in 0..self.image.width() {
                let old_c = canvas.drawing_buffer.get(y, x);
                let c = self.image.get(y, x);

                // TODO: The second cast should be a round!
                let c = (c.cast::<f32>() * paint.alpha + old_c.cast::<f32>() * (1. - paint.alpha))
                    .cast::<u8>();

                canvas.drawing_buffer.set(y, x, &Color::from(c));
            }
        }

        Ok(())
    }
}

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
