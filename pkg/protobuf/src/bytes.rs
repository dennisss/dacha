use common::const_default::ConstDefault;

/// Used to represent the 'bytes' type in memory.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BytesField(pub Vec<u8>);

impl std::ops::Deref for BytesField {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for BytesField {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ConstDefault for BytesField {
    const DEFAULT: Self = BytesField(Vec::new());
}