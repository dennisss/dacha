
#[derive(Clone, Copy, Debug)]
pub struct CursorRange {
    pub start: usize,
    pub end: usize,
}

impl CursorRange {
    pub fn zero() -> Self {
        Self { start: 0, end: 0 }
    }

    pub fn update(&mut self, idx: usize, holding_shift: bool) {
        self.end = idx;
        if !holding_shift {
            self.start = self.end;
        }
    }
}