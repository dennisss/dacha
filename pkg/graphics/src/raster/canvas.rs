use common::errors::*;
use common::iter::PairIter;
use image::{Color, Colorspace, Image};
use math::matrix::{vec2f, Matrix3f, Vector2f, Vector2i, Vector3f};

use crate::canvas::base::CanvasBase;
use crate::canvas::curve::Curve;
use crate::canvas::path::*;
use crate::canvas::Canvas;
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
    fn fill_path(&mut self, path: &Path, color: &Color) -> Result<()> {
        let (verts, path_starts) = path.linearize(self.base.current_transform());

        // TODO: Currently this assumes that all subpaths are closed. Possibly we should
        // instead Optimize this more and not use all of the empty line segments
        // between lines.
        crate::raster::fill_polygon(
            &mut self.drawing_buffer,
            &verts,
            color,
            &path_starts,
            FillRule::NonZero,
        )?;
        Ok(())
    }

    /// TODO: Must use the non-zero winding rule for this always.
    /// TODO: This has a lot of redundant computation with fill_path if we ever
    /// want to do both.
    fn stroke_path(&mut self, path: &Path, width: f32, color: &Color) -> Result<()> {
        let (verts, path_starts) = path.linearize(self.base.current_transform());

        let scale = self.base.current_transform()[(0, 0)];
        let width_scaled = width * scale;

        let dash_array = &[]; // &[5.0 * scale, 5.0 * scale];

        for (i, j) in path_starts.pair_iter() {
            let dashes = crate::raster::stroke::stroke_split_dashes(&verts[*i..*j], dash_array);

            for dash in dashes {
                let points = crate::raster::stroke::stroke_poly(&dash, width_scaled);
                let starts = &[0, points.len()];

                crate::raster::fill_polygon(
                    &mut self.drawing_buffer,
                    &points,
                    color,
                    starts,
                    FillRule::NonZero,
                )?;
            }
        }

        Ok(())
    }

    fn draw_image(&mut self, image: &Image<u8>) -> Result<()> {
        let white = Color::rgb(255, 255, 255);

        for y in 0..image.height() {
            for x in 0..image.width() {
                let c = image.get(y, x);

                // TODO: The second cast should be a round!
                let c = (c.cast::<f32>() * 0.2 + white.cast::<f32>() * 0.8).cast::<u8>();

                self.drawing_buffer.set(y, x, &Color::from(c));
            }
        }

        Ok(())
    }

    // pub fn clear() {}
}

// Implementing

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
