use graphics::canvas::Path;

use crate::syntax::Polarity;

/// Single primitive to be drawn to represent a Gerber file. These should be
/// drawn in the order they are generated.
///
/// Gerber files need to be drawn as a set of layers where each layer is an
/// independent drawing buffer. At the end, all layers can be flattened with the
/// color of later (higher) layers overriding earlier (lower) layers.
#[derive(Clone, Debug)]
pub enum GraphicsObject {
    // TODO: Need to normalize the paths to have only counter-clockwise geometry.
    FillPath(Path, FillMode),
    EndOfLayer,
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
