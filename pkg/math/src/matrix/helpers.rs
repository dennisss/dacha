use crate::matrix::base::Vector2f;

#[inline]
pub fn vec2f(x: f32, y: f32) -> Vector2f {
    Vector2f::from_slice(&[x, y])
}
