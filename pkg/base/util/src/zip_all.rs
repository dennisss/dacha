
pub struct ZipAllIterator<A, B> {
    a: A,
    b: B,
}

impl<T, Y, A: Iterator<Item = T>, B: Iterator<Item = Y>> ZipAllIterator<A, B> {
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<T, Y, A: Iterator<Item = T>, B: Iterator<Item = Y>> Iterator for ZipAllIterator<A, B> {
    type Item = (Option<T>, Option<Y>);

    fn next(&mut self) -> Option<Self::Item> {
        let a = self.a.next();
        let b = self.b.next();
        if a.is_none() && b.is_none() {
            return None;
        }

        Some((a, b))
    }
}