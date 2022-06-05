use common::errors::*;
use math::matrix::{Matrix3f, Vector2f};

pub struct CanvasBase {
    transform: Matrix3f,
    transform_stack: Vec<Matrix3f>,
}

impl CanvasBase {
    pub fn new() -> Self {
        Self {
            transform: Matrix3f::identity(),
            transform_stack: vec![],
        }
    }

    pub fn current_transform(&self) -> &Matrix3f {
        &self.transform
    }

    pub fn save(&mut self) {
        self.transform_stack.push(self.transform.clone());
    }

    pub fn restore(&mut self) -> Result<()> {
        self.transform = self
            .transform_stack
            .pop()
            .ok_or_else(|| err_msg("No transforms saved on the stack"))?;
        Ok(())
    }

    pub fn scale(&mut self, x: f32, y: f32) {
        self.transform =
            &self.transform * crate::transforms::scale2f(&Vector2f::from_slice(&[x, y]));
    }

    pub fn translate(&mut self, x: f32, y: f32) {
        self.transform =
            &self.transform * crate::transforms::translate2f(Vector2f::from_slice(&[x, y]));
    }
}
