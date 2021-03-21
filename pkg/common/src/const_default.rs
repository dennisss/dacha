

pub trait ConstDefault {
    const DEFAULT: Self;
}

macro_rules! impl_const_default {
    ($t:ident, $v:expr) => {
        impl ConstDefault for $t {
            const DEFAULT: Self = $v;
        }
    };
}

impl_const_default!(u8, 0);
impl_const_default!(i8, 0);
impl_const_default!(u16, 0);
impl_const_default!(i16, 0);
impl_const_default!(u32, 0);
impl_const_default!(i32, 0);
impl_const_default!(u64, 0);
impl_const_default!(i64, 0);
impl_const_default!(usize, 0);
impl_const_default!(isize, 0);
impl_const_default!(f32, 0.0);
impl_const_default!(f64, 0.0);
impl_const_default!(String, String::new());

impl<T> ConstDefault for Option<T> {
    const DEFAULT: Self = None;
}

impl<T> ConstDefault for Vec<T> {
    const DEFAULT: Self = Vec::new();
}