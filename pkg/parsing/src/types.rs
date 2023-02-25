use common::errors::*;

use crate::incomplete_error;

/// A type which can be represented as bytes in one canonical way.
pub trait BinaryRepr {
    const SIZE_OF: Option<usize>;

    fn parse_from_bytes<'a>(input: &'a [u8], endian: Endian) -> Result<(Self, &'a [u8])>
    where
        Self: Sized;
}

#[derive(Clone, Copy, Debug)]
pub enum Endian {
    Little,
    Big,
}

pub const fn add_size_of(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    let a = match a {
        Some(v) => v,
        None => return None,
    };

    let b = match b {
        Some(v) => v,
        None => return None,
    };

    Some(a + b)
}

macro_rules! primitive_repr {
    ($t:ty) => {
        impl BinaryRepr for $t {
            const SIZE_OF: Option<usize> = Some(::core::mem::size_of::<$t>());

            fn parse_from_bytes(input: &[u8], endian: Endian) -> Result<(Self, &[u8])> {
                const LEN: usize = core::mem::size_of::<$t>();
                if input.len() < LEN {
                    return Err(incomplete_error(input.len()));
                }

                let v = <$t>::from_le_bytes(*array_ref![input, 0, LEN]);
                Ok((v, &input[LEN..]))
            }
        }
    };
}

primitive_repr!(u8);
primitive_repr!(i8);

primitive_repr!(u16);
primitive_repr!(i16);
primitive_repr!(u32);
primitive_repr!(i32);
primitive_repr!(u64);
primitive_repr!(i64);
primitive_repr!(f32);
primitive_repr!(f64);
