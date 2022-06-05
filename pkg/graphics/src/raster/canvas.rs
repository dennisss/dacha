use common::errors::*;
use common::iter::PairIter;
use image::{Color, Colorspace, Image};
use math::matrix::{vec2f, Matrix3f, Vector2f, Vector2i, Vector3f};

use crate::canvas::base::CanvasBase;
use crate::canvas::bezier::BezierCurve;
use crate::canvas::curve::Curve;
use crate::canvas::ellipse::Ellipse;
use crate::canvas::path::*;
use crate::raster::FillRule;
use crate::transforms::transform2f;

pub struct Canvas {
    // TODO: Make private.
    pub drawing_buffer: Image<u8>,
    base: CanvasBase,
}

impl_deref!(Canvas::base as CanvasBase);

impl Canvas {
    pub fn create(height: usize, width: usize) -> Self {
        Self {
            drawing_buffer: Image::zero(height, width, Colorspace::RGB),
            base: CanvasBase::new(),
        }
    }

    pub fn fill_path(&mut self, path: &Path, color: &Color) -> Result<()> {
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
    pub fn stroke_path(&mut self, path: &Path, width: f32, color: &Color) -> Result<()> {
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

    pub fn fill_rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: &Color,
    ) -> Result<()> {
        let mut rect = PathBuilder::new();
        rect.move_to(Vector2f::from_slice(&[x, y]));
        rect.line_to(Vector2f::from_slice(&[x + width, y]));
        rect.line_to(Vector2f::from_slice(&[x + width, y + height]));
        rect.line_to(Vector2f::from_slice(&[x, y + height]));
        rect.close();
        self.fill_path(&rect.build(), color)
    }

    pub fn stroke_rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        line_width: f32,
        color: &Color,
    ) -> Result<()> {
        let mut rect = PathBuilder::new();
        rect.move_to(Vector2f::from_slice(&[x, y]));
        rect.line_to(Vector2f::from_slice(&[x + width, y]));
        rect.line_to(Vector2f::from_slice(&[x + width, y + height]));
        rect.line_to(Vector2f::from_slice(&[x, y + height]));
        rect.close();
        self.stroke_path(&rect.build(), line_width, color)
    }

    pub fn clear() {}
}

// Implementing

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
