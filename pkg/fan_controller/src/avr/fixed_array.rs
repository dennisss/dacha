#[macro_export]
macro_rules! define_fixed_array {
    ($name:ident, $ty:ident, $max_length:expr) => {
        struct $name {
            values: [Option<$ty>; $max_length],
            length: usize,
        }
        impl $name {
            const fn new() -> Self {
                Self {
                    values: [None; $max_length],
                    length: 0,
                }
            }
            fn len(&self) -> usize {
                self.length
            }
            #[inline(never)]
            fn push(&mut self, value: $ty) {
                // NOTE: Will panic if too many values are enqueued.
                self.values[self.length] = Some(value);
                self.length += 1;
            }
            fn swap_remove(&mut self, index: usize) {
                assert!(index < self.length);
                if index == self.length - 1 {
                    self.values[index] = None;
                } else {
                    self.values[index] = self.values[self.length - 1].take();
                }
                self.length -= 1;
            }
            fn index(&self, idx: usize) -> &$ty {
                assert!(idx < self.len());
                self.values[idx].as_ref().unwrap()
            }
        }
    };
}
