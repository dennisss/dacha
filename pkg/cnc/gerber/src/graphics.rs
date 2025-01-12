use graphics::{canvas::Path, transforms::transform2f};
use math::matrix::{Matrix3f, Vector2f};

use crate::syntax::Polarity;

/// Single primitive to be drawn to represent a Gerber file. These and the
/// nested paths should be drawn in the order they are generated.
///
/// Gerber files need to be drawn as a set of layers where each layer is an
/// independent drawing buffer. Each 'GraphicsObject' should be drawn as a
/// separate layer. At the end, all layers can be flattened with the
/// color of later (higher) layers overriding earlier (lower) layers.
#[derive(Clone, Debug)]
pub struct GraphicsObject {
    pub paths: Vec<GraphicsPath>,

    pub line: Option<(Vector2f, Vector2f)>,
}

impl GraphicsObject {
    pub fn transform(&mut self, transform: &Matrix3f) {
        for path in &mut self.paths {
            path.path.transform(transform);
        }

        if let Some((start, end)) = &mut self.line {
            *start = transform2f(transform, start);
            *end = transform2f(transform, end);
        }
    }
}

/// A single path that needs to be filled to draw part of the Gerber file.
#[derive(Clone, Debug)]
pub struct GraphicsPath {
    pub path: Path,
    pub fill: FillMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillMode {
    /// Make the color 'dark' in the current layer. Should override lower
    /// layers.
    Dark,

    /// Make the color 'clear' in the current layer. Should override lower
    /// layers.
    Clear,

    /// Clear any color data within the path in the current layer. The final
    /// color value is the same as the lower layers (this is used to implement
    /// non-exposured objects).
    Unset,
}

impl From<Polarity> for FillMode {
    fn from(value: Polarity) -> Self {
        match value {
            Polarity::Clear => FillMode::Clear,
            Polarity::Dark => FillMode::Dark,
        }
    }
}
