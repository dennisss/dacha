/// Stores data indexed by (i, j) where i < j in a flat vec.
#[derive(Default)]
pub struct TupleVec<T> {
    num: usize,
    data: Vec<T>
}

impl<T: Clone> TupleVec<T> {
    /// 'num' is the max value of the index + 1
    pub fn new(default_value: T, num: usize) -> Self {
        let size = (num * (num - 1)) / 2;
        Self {
            num,
            data: vec![default_value; size],
        }
    }

    pub fn set(&mut self, i: usize, j: usize, value: T) {
        let idx = self.get_index(i, j);
        self.data[idx] = value;
    }

    pub fn get(&self, i: usize, j: usize) -> &T {
        let idx = self.get_index(i, j);
        &self.data[idx]
    }

    #[inline]
    fn get_index(&self, mut i: usize, mut j: usize) -> usize {
        assert!(i < j);
        assert!(j < self.num);

        (i * self.num) - (i * (i + 1) / 2) + j - i - 1
    }    
}
