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

/*
We have a few options for representing paths:
- Either, store in the Canvas and pass in an Id
    - Allows having a simpler type to reference objects and can have a biffer
- It's still useful to

Other challenges:
- For fonts, we need to be able to cache at multiple scales (for now just cache different )
- Means that we require



*/

impl Canvas for RasterCanvas {
    fn create_path_fill(&mut self, path: &Path) -> Result<Box<dyn CanvasObject>> {
        // TODO: Currently this assumes that all subpaths are closed. Possibly we should
        // instead Optimize this more and not use all of the empty line segments
        // between lines.
        Ok(Box::new(RasterCanvasPath {
            path: path.clone(),
            usage: PathUsage::Fill,
            data: None,
        }))
    }

    /// TODO: Must use the non-zero winding rule for this always.
    /// TODO: This has a lot of redundant computation with fill_path if we ever
    /// want to do both.
    fn create_path_stroke(&mut self, path: &Path, width: f32) -> Result<Box<dyn CanvasObject>> {
        Ok(Box::new(RasterCanvasPath {
            path: path.clone(),
            usage: PathUsage::Stroke { width },
            data: None,
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
    path: Path,
    usage: PathUsage,
    data: Option<CachedPathData>,
}

struct CachedPathData {
    transform_inv: Matrix3f,
    vertices: Vec<Vector2f>,
    path_starts: Vec<usize>,
}

impl RasterCanvasPath {
    fn data<'a>(&'a mut self, canvas: &RasterCanvas) -> &'a mut CachedPathData {
        let transform = canvas.current_transform();

        if let Some(existing_data) = self.data.as_mut() {
            if !self
                .path
                .can_reuse_linearized(transform, &existing_data.transform_inv)
            {
                self.data = None;
            }
        }

        // NOTE: This code is organized this way to avoid NLL bugs.
        if let Some(ref mut existing_data) = self.data {
            return existing_data;
        }

        self.recompute(canvas)
    }

    fn recompute(&mut self, canvas: &RasterCanvas) -> &mut CachedPathData {
        let mut transform = canvas.current_transform();

        let (vertices, path_starts) = match self.usage {
            PathUsage::Fill => self.path.linearize(transform),
            PathUsage::Stroke { width } => self.path.stroke(width, transform),
        };

        self.data.insert(CachedPathData {
            transform_inv: transform.inverse(),
            vertices,
            path_starts,
        })
    }
}

impl CanvasObject for RasterCanvasPath {
    fn draw(&mut self, paint: &crate::canvas::Paint, canvas: &mut dyn Canvas) -> Result<()> {
        let canvas = canvas.as_mut_any().downcast_mut::<RasterCanvas>().unwrap();

        let data = self.data(canvas);

        // Undo the transform used for linearization and apply the current transform.
        let mut vertices = data.vertices.clone();
        for v in &mut vertices {
            *v = transform2f(&(canvas.current_transform() * &data.transform_inv), v);
        }

        // TODO: Implement alpha mixing.

        crate::raster::fill_polygon(
            &mut canvas.drawing_buffer,
            &vertices,
            &paint.color,
            &data.path_starts,
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
