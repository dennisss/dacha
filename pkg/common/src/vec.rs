#[derive(Clone)]
pub enum VecPtr<T: 'static> {
    Static(&'static [T]),
    Dynamic(Vec<T>),
}

impl<T: std::fmt::Debug + Clone> VecPtr<T> {
    pub fn new() -> Self {
        Self::Dynamic(vec![])
    }

    pub fn from(data: &[T]) -> Self {
        Self::from_vec(data.to_vec())
    }

    pub fn from_vec(data: Vec<T>) -> Self {
        Self::Dynamic(data)
    }

    pub const fn from_static(data: &'static [T]) -> Self {
        Self::Static(data)
    }

    pub fn push(&mut self, value: T) {
        self.as_mut().push(value)
    }

    pub fn pop(&mut self) -> Option<T> {
        self.as_mut().pop()
    }

    pub fn extend_from_slice(&mut self, other: &[T]) {
        self.as_mut().extend_from_slice(other);
    }

    pub fn last(&self) -> Option<&T> {
        self.as_ref().last()
    }

    pub fn resize(&mut self, new_len: usize, value: T) {
        self.as_mut().resize(new_len, value);
    }

    pub fn len(&self) -> usize {
        self.as_ref().len()
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for VecPtr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}",
            match self {
                Self::Static(v) => v,
                Self::Dynamic(v) => &v[..],
            }
        )
    }
}

impl<T: std::fmt::Debug + Clone + std::cmp::PartialEq> std::cmp::PartialEq for VecPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<T: std::fmt::Debug + Clone + std::cmp::PartialEq> std::cmp::Eq for VecPtr<T> {}

impl<T: std::fmt::Debug + Clone> std::convert::AsRef<[T]> for VecPtr<T> {
    fn as_ref(&self) -> &[T] {
        match self {
            Self::Static(v) => v,
            Self::Dynamic(v) => v.as_ref(),
        }
    }
}

impl<T: std::fmt::Debug + Clone> std::convert::AsMut<Vec<T>> for VecPtr<T> {
    fn as_mut(&mut self) -> &mut Vec<T> {
        if let Self::Static(v) = self {
            let arr = v.to_vec();
            *self = Self::Dynamic(arr);
        }

        match self {
            Self::Dynamic(v) => v,
            Self::Static(v) => panic!(""),
        }
    }
}

impl<T: std::fmt::Debug + Clone> std::ops::Index<usize> for VecPtr<T> {
    type Output = T;
    fn index(&self, idx: usize) -> &Self::Output {
        &self.as_ref()[idx]
    }
}

impl<T: std::fmt::Debug + Clone> std::ops::IndexMut<usize> for VecPtr<T> {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.as_mut()[idx]
    }
}
