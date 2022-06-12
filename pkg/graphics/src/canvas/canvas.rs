use core::ops::{Deref, DerefMut};
use std::any::Any;

use common::errors::*;
use image::{Color, Image};
use math::matrix::vec2f;

use crate::canvas::base::CanvasBase;
use crate::canvas::path::{Path, PathBuilder};

/// Interface for 2D rendering to a rectangular screen area.
///
/// All operations consider the top-left corner to be (0, 0).
pub trait Canvas: Deref<Target = CanvasBase> + DerefMut {
    fn clear_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: &Color) -> Result<()> {
        self.fill_rectangle(x, y, width, height, color)
    }

    fn fill_path(&mut self, path: &Path, color: &Color) -> Result<()>;

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
        self.fill_path(&path.build(), color)
    }

    fn stroke_path(&mut self, path: &Path, width: f32, color: &Color) -> Result<()>;

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
        self.stroke_path(&path.build(), line_width, color)
    }

    // /// Ingests an
    fn load_image(&mut self, image: &Image<u8>) -> Result<Box<dyn Any>>;

    fn draw_image(&mut self, image: &dyn Any, alpha: f32) -> Result<()>;
}
