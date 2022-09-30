pub trait Number {}

pub trait Min {
    fn min(self, other: Self) -> Self;
}

impl<T: PartialOrd> Min for T {
    fn min(self, other: Self) -> Self {
        if self <= other {
            self
        } else {
            other
        }
    }
}

pub trait Max {
    fn max(self, other: Self) -> Self;
}

impl<T: PartialOrd> Max for T {
    fn max(self, other: Self) -> Self {
        if self >= other {
            self
        } else {
            other
        }
    }
}

pub trait Zero {
    fn zero() -> Self;

    fn is_zero(&self) -> bool;
}

pub trait One {
    fn one() -> Self;

    fn is_one(&self) -> bool;
}

pub trait AbsoluteValue {
    fn abs(self) -> Self;
}

/// A type which can be explicitly coerced into another type possibly with
/// precision loss.
///
/// 'self as T' === self.cast()
pub trait Cast<T> {
    fn cast(self) -> T;
}

macro_rules! impl_num_type {
    ($name:ty, $sign:ident) => {
        impl Number for $name {}

        impl Zero for $name {
            fn zero() -> Self {
                0 as $name
            }

            fn is_zero(&self) -> bool {
                *self == Self::zero()
            }
        }

        impl One for $name {
            fn one() -> Self {
                1 as $name
            }

            fn is_one(&self) -> bool {
                *self == Self::one()
            }
        }

        impl_cast!(i8, $name);
        impl_cast!(u8, $name);
        impl_cast!(i16, $name);
        impl_cast!(u16, $name);
        impl_cast!(i32, $name);
        impl_cast!(u32, $name);
        impl_cast!(i64, $name);
        impl_cast!(u64, $name);
        impl_cast!(isize, $name);
        impl_cast!(usize, $name);
        impl_cast!(f32, $name);
        impl_cast!(f64, $name);

        impl_num_abs!($name, $sign);
    };
}

macro_rules! impl_cast {
    ($a:ty, $b:ty) => {
        impl Cast<$a> for $b {
            fn cast(self) -> $a {
                self as $a
            }
        }
    };
}

macro_rules! impl_num_abs {
    ($name:ty, I) => {
        impl AbsoluteValue for $name {
            fn abs(self) -> Self {
                <$name>::abs(self)
            }
        }
    };
    ($name:ty, U) => {
        impl AbsoluteValue for $name {
            fn abs(self) -> Self {
                self
            }
        }
    };
}

impl_num_type!(i8, I);
impl_num_type!(u8, U);
impl_num_type!(i16, I);
impl_num_type!(u16, U);
impl_num_type!(i32, I);
impl_num_type!(u32, U);
impl_num_type!(i64, I);
impl_num_type!(u64, U);
impl_num_type!(isize, I);
impl_num_type!(usize, U);
impl_num_type!(f32, I);
impl_num_type!(f64, I);

pub trait CastTo = Sized + Cast<i8> + Cast<u8> + Cast<i32> + Cast<i64> where i64: Cast<Self>;

pub trait Float: From<i8> + From<i16> + From<f32> + CastTo {
    fn sqrt(self) -> Self;
    fn round(self) -> Self;
}

macro_rules! impl_float_type {
    ($name:ty) => {
        impl Float for $name {
            fn sqrt(self) -> Self {
                <$name>::sqrt(self)
            }

            fn round(self) -> Self {
                <$name>::round(self)
            }
        }
    };
}

impl_float_type!(f32);
impl_float_type!(f64);
