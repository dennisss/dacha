use core::ops::{Deref, DerefMut};

pub struct Aligned<Data, Alignment> {
    aligner: [Alignment; 0],
    data: Data,
}

impl<Data, Alignment> Aligned<Data, Alignment> {
    pub fn new(data: Data) -> Self {
        Self { aligner: [], data }
    }
}

impl<Data, Alignment> Deref for Aligned<Data, Alignment> {
    type Target = Data;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<Data, Alignment> DerefMut for Aligned<Data, Alignment> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}
