use crate::matrix::base::{Vector2f, Vector2, Vector3f};
use crate::matrix::element::ElementType;

#[inline]
pub fn vec2f(x: f32, y: f32) -> Vector2f {
    Vector2f::from_slice(&[x, y])
}

#[inline]
pub fn vec3f(x: f32, y: f32, z: f32) -> Vector3f {
    Vector3f::from_slice(&[x, y, z])
}

#[macro_export]
macro_rules! vecxf {
    ($( $x:expr ),* $(,)?) => {{
        let values = [ $( $x ),* ];
        $crate::matrix::VectorXf::from_slice_with_shape(values.len(), 1, &values)
    }};
}

#[macro_export]
macro_rules! vecxd {
    ($( $x:expr ),* $(,)?) => {{
        let values = [ $( $x ),* ];
        $crate::matrix::VectorXd::from_slice_with_shape(values.len(), 1, &values)
    }};
}

#[inline]
pub fn vec2<T: ElementType>(x: T, y: T) -> Vector2<T> {
    Vector2::from_slice(&[x, y])
}