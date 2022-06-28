
pub struct ConcatSlicePair<'a> {
    a: &'a [u8],
    b: &'a [u8]
}

impl<'a> ConcatSlicePair<'a> {
    pub fn new(a: &'a [u8], b: &'a [u8]) -> Self {
        Self { a, b }
    }

    pub fn read(&mut self, mut out: &mut [u8]) -> usize {
        let mut total = 0;
        total += Self::read_from_slice(&mut self.a, &mut out);
        total += Self::read_from_slice(&mut self.b, &mut out);
        total
    }

    pub fn read_from_slice(input: &mut &'a [u8], output: &mut &mut [u8]) -> usize {
        let n = input.len().min(output.len());
        (*output)[0..n].copy_from_slice(&(*input)[0..n]);
        *input = &(*input)[n..];

        output.take_mut(..n);
        // *output = &mut (*output)[n..];
        n
    }

    pub fn len(&self) -> usize {
        self.a.len() + self.b.len()
    } 

}
