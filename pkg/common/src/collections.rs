use generic_array::{ArrayLength, GenericArray};

pub struct FixedArray<T, N: ArrayLength<T>> {
    data: GenericArray<T, N>,
}
