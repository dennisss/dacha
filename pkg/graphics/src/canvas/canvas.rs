use core::ops::{Deref, DerefMut};

use common::errors::*;
use image::{Color, Image};
use math::matrix::vec2f;

use crate::canvas::base::CanvasBase;
use crate::canvas::path::{Path, PathBuilder};

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
        let mut rect = PathBuilder::new();
        // TODO: Move this to the PathBuilder.
        rect.move_to(vec2f(x, y));
        rect.line_to(vec2f(x + width, y));
        rect.line_to(vec2f(x + width, y + height));
        rect.line_to(vec2f(x, y + height));
        rect.close();
        self.fill_path(&rect.build(), color)
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
        let mut rect = PathBuilder::new();
        rect.move_to(vec2f(x, y));
        rect.line_to(vec2f(x + width, y));
        rect.line_to(vec2f(x + width, y + height));
        rect.line_to(vec2f(x, y + height));
        rect.close();
        self.stroke_path(&rect.build(), line_width, color)
    }

    // /// Ingests an
    // fn load_image(&mut self, image: Image<u8>) -> Result<Box<dyn CanvasImage>>;

    fn draw_image(&mut self, image: &Image<u8>) -> Result<()>;
}

// pub trait CanvasImage {}
