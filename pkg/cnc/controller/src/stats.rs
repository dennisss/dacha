use std::fmt::Debug;

#[derive(Default, Debug)]
pub struct MinMaxStats<T> {
    valid: bool,
    min: T,
    max: T
}

impl<T: Copy + PartialOrd> MinMaxStats<T> {
    pub fn add(&mut self, value: T) {
        if !self.valid {
            self.min = value;
            self.max = value;
            self.valid = true;
            return;
        }

        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
    }

    pub fn max(&self) -> T {
        self.max
    }

    pub fn min(&self) -> T {
        self.min
    }
}

impl<T: Copy + Debug + std::ops::Sub<Output = T>> MinMaxStats<T> {
    pub fn print(&self) -> String {
        format!("[min: {:?}; max: {:?}; range: {:?}]", self.min, self.max, self.max - self.min)
    }
}