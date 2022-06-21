use crate::matrix::base::{Vector2f, Vector2};
use crate::matrix::element::ElementType;

#[inline]
pub fn vec2f(x: f32, y: f32) -> Vector2f {
    Vector2f::from_slice(&[x, y])
}

#[inline]
pub fn vec2<T: ElementType>(x: T, y: T) -> Vector2<T> {
    Vector2::from_slice(&[x, y])
}