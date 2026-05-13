
/// Utility for doing binary search either to find an exact value or to find the
/// smallest / larger feasible value. 
pub struct BinarySearch {
    min: usize,
    max: usize,
    current: usize,
    best: Option<usize>,
    done: bool,
}

impl BinarySearch {
    /// Initiates a search over all values in the range [min, max]
    /// (inclusive of both).
    pub fn new(min: usize, max: usize) -> Self {
        Self {
            min,
            max,
            current: (min + max) / 2,
            best: None,
            done: false
        }
    }

    pub fn done(&self) -> bool {
        self.done
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn best(&self) -> Option<usize> {
        self.best
    }

    pub fn greater_eq_current(&mut self) {
        self.best = Some(self.current);
        self.greater_than_current();
    }

    pub fn greater_than_current(&mut self) {
        if self.current == self.max {
            self.done = true;
            return;
        }

        self.min = self.current + 1;
        self.current = (self.min + self.max) / 2;
    }

    pub fn less_eq_current(&mut self) {
        self.best = Some(self.current);
        self.less_than_current();
    }

    pub fn less_than_current(&mut self) {
        if self.current == self.min {
            self.done = true;
            return;
        }

        self.max = self.current - 1;
        self.current = (self.min + self.max) / 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {
        for i in 0..11 {
            let mut s = BinarySearch::new(0, 10);

            while !s.done() {
                if i >= s.current() {
                    s.greater_eq_current();
                } else {
                    s.less_than_current();
                }
            }

            assert_eq!(s.best().unwrap(), i);
        }

        for i in 0..11 {
            let mut s = BinarySearch::new(0, 10);

            while !s.done() {
                if i <= s.current() {
                    s.less_eq_current();
                } else {
                    s.greater_than_current();
                }
            }

            assert_eq!(s.best().unwrap(), i);
        }
    }

}
