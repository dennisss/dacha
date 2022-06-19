use core::ops::{Deref, DerefMut};
use std::any::Any;

use common::errors::*;
use image::{Color, Image};
use math::matrix::vec2f;

use crate::canvas::base::CanvasBase;
use crate::canvas::path::{Path, PathBuilder};

pub trait AsAny {
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_mut_any(&mut self) -> &mut dyn Any {
        self
    }
}

/// Interface for 2D rendering to a rectangular screen area.
///
/// All operations consider the top-left corner to be (0, 0).
pub trait Canvas: Deref<Target = CanvasBase> + DerefMut + AsAny {
    fn create_path_fill(&mut self, path: &Path) -> Result<Box<dyn CanvasObject>>;

    fn create_path_stroke(&mut self, path: &Path, width: f32) -> Result<Box<dyn CanvasObject>>;

    /// Ingests an
    fn create_image(&mut self, image: &Image<u8>) -> Result<Box<dyn CanvasObject>>;

    // fn draw_image(&mut self, image: &dyn Any, alpha: f32) -> Result<()>;
}

pub trait CanvasHelperExt {
    fn clear_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: &Color) -> Result<()> {
        self.fill_rectangle(x, y, width, height, color)
    }

    fn fill_path(&mut self, path: &Path, color: &Color) -> Result<()>;

    fn stroke_path(&mut self, path: &Path, width: f32, color: &Color) -> Result<()>;

    fn fill_rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: &Color,
    ) -> Result<()>;

    fn stroke_rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        line_width: f32,
        color: &Color,
    ) -> Result<()>;
}

impl CanvasHelperExt for dyn Canvas + '_ {
    fn fill_path(&mut self, path: &Path, color: &Color) -> Result<()> {
        self.create_path_fill(path)?.draw(
            &Paint {
                color: color.clone(),
                alpha: 1.,
            },
            self,
        )
    }

    fn stroke_path(&mut self, path: &Path, width: f32, color: &Color) -> Result<()> {
        self.create_path_stroke(path, width)?.draw(
            &Paint {
                color: color.clone(),
                alpha: 1.,
            },
            self,
        )
    }

    fn fill_rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: &Color,
    ) -> Result<()> {
        let mut path = PathBuilder::new();
        path.rect(x, y, width, height);

        self.create_path_fill(&path.build())?.draw(
            &Paint {
                color: color.clone(),
                alpha: 1.,
            },
            self,
        )
    }

    fn stroke_rectangle(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        line_width: f32,
        color: &Color,
    ) -> Result<()> {
        let mut path = PathBuilder::new();
        path.rect(x, y, width, height);

        self.create_path_stroke(&path.build(), line_width)?.draw(
            &Paint {
                color: color.clone(),
                alpha: 1.,
            },
            self,
        )
    }
}

pub trait CanvasObject {
    /// NOTE: It is only valid to draw an object on the same canvas that created
    /// it.
    fn draw(&mut self, paint: &Paint, canvas: &mut dyn Canvas) -> Result<()>;
}

pub struct Paint {
    pub color: Color,
    pub alpha: f32,
}

impl Paint {
    pub fn alpha(value: f32) -> Self {
        Self {
            color: Color::rgb(255, 255, 255),
            alpha: value,
        }
    }
}
