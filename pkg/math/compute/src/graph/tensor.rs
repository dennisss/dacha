use core::fmt::Debug;
use core::ops::Deref;
use std::sync::Arc;

use math::array::Array;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    Float32,
    Uint8,
    Uint32,
}

impl DataType {
    pub fn short_name(&self) -> &'static str {
        match self {
            DataType::Float32 => "f32",
            DataType::Uint8 => "u8",
            DataType::Uint32 => "u32",
        }
    }
}

/// Reference to a stream of n-dimensional arrays that are the source/sink of
/// some computation.
#[derive(Clone)]
pub struct Tensor {
    array: Arc<TensorArray>,
}

impl Debug for Tensor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.array)
    }
}

impl<T: Into<TensorArray>> From<T> for Tensor {
    fn from(value: T) -> Self {
        Self {
            array: Arc::new(value.into()),
        }
    }
}

impl Deref for Tensor {
    type Target = TensorArray;

    fn deref(&self) -> &Self::Target {
        self.array.as_ref()
    }
}

#[macro_export]
macro_rules! tensor_array_do {
    ($arr:expr, $v:ident, $e:expr) => {
        match $arr {
            TensorArray::Float32($v) => $e,
            TensorArray::Uint32($v) => $e,
            TensorArray::Uint8($v) => $e,
        }
    };
}

impl Tensor {
    pub fn shape(&self) -> &[usize] {
        tensor_array_do!(self.array.as_ref(), v, &v.shape[..])
    }

    pub fn size(&self) -> usize {
        tensor_array_do!(self.array.as_ref(), v, v.size())
    }

    pub fn cast(&self, dtype: DataType) -> Tensor {
        if self.dtype() == dtype {
            return self.clone();
        }

        let arr = self.array.as_ref();
        match dtype {
            DataType::Float32 => tensor_array_do!(arr, v, v.cast::<f32>().into()),
            DataType::Uint8 => tensor_array_do!(arr, v, v.cast::<u8>().into()),
            DataType::Uint32 => tensor_array_do!(arr, v, v.cast::<u32>().into()),
        }
    }
}

pub enum TensorArray {
    Float32(Array<f32>),
    Uint8(Array<u8>),
    Uint32(Array<u32>),
}

impl Debug for TensorArray {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let inner = tensor_array_do!(self, v, format!("{:?}", v));
        write!(f, "{}{}", self.dtype().short_name(), inner)
    }
}

impl<T: TensorArrayType> From<T> for TensorArray {
    fn from(value: T) -> Self {
        T::scalar_array(value)
    }
}

impl<T: TensorArrayType> From<Array<T>> for TensorArray {
    fn from(value: Array<T>) -> Self {
        T::upcast_array(value)
    }
}

impl TensorArray {
    pub fn dtype(&self) -> DataType {
        match self {
            Self::Float32(_) => DataType::Float32,
            Self::Uint8(_) => DataType::Uint8,
            Self::Uint32(_) => DataType::Uint32,
        }
    }

    pub fn array<T: TensorArrayType>(&self) -> Option<&Array<T>> {
        T::downcast_array(self)
    }
}

pub trait TensorArrayType {
    fn scalar_array(value: Self) -> TensorArray
    where
        Self: Sized;

    fn downcast_array(array: &TensorArray) -> Option<&Array<Self>>
    where
        Self: Sized;

    fn upcast_array(array: Array<Self>) -> TensorArray
    where
        Self: Sized;
}

macro_rules! tensor_array_type {
    ($variant:ident, $t:ty) => {
        impl TensorArrayType for $t {
            fn scalar_array(value: Self) -> TensorArray {
                TensorArray::$variant(Array::scalar(value))
            }

            fn downcast_array(array: &TensorArray) -> Option<&Array<Self>> {
                match array {
                    TensorArray::$variant(v) => Some(v),
                    _ => None,
                }
            }

            fn upcast_array(array: Array<Self>) -> TensorArray {
                TensorArray::$variant(array)
            }
        }
    };
}

tensor_array_type!(Float32, f32);
tensor_array_type!(Uint32, u32);
tensor_array_type!(Uint8, u8);
