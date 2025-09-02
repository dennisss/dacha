
pub struct Histogram {
    boundaries: Vec<f32>,
    bins: Vec<u64>
}

impl Histogram {
    pub fn uniform_boundaries(mut start: f32, interval: f32, end: f32) -> Vec<f32> {
        assert!(start <= end);

        let mut boundaries = vec![];

        while start <= end {
            // TODO: Make ths rounding more configurable
            boundaries.push((start * 10000.0).floor() / 10000.0);
            start += interval;
        }

        boundaries
    }

    pub fn new(boundaries: Vec<f32>) -> Self {
        assert!(boundaries.len() >= 1);
        for i in 1..boundaries.len() {
            assert!(boundaries[i] > boundaries[i - 1]);
        }

        let bins = vec![0; boundaries.len() + 1];

        Self {
            boundaries,
            bins,
        }
    }

    pub fn increment(&mut self, value: f32) {
        let bin_index = common::algorithms::upper_bound(&self.boundaries, &value)
            .map(|v| v + 1)
            .unwrap_or(0);
        
        self.bins[bin_index] += 1;
    }

    pub fn print(&self) {
        for i in 0..self.bins.len() {
            let label = {
                if i == 0 {
                    format!("[-inf, {})", self.boundaries[0])
                } else if i == self.bins.len() - 1 {
                    format!("[{}, inf]", self.boundaries[i - 1])
                } else {
                    format!("[{}, {})", self.boundaries[i - 1], self.boundaries[i])
                }
            };

            println!("{} :\t{}", label, self.bins[i]);
        }
    }

}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn works() {
        let mut hist = Histogram::new(Histogram::uniform_boundaries(0.0, 0.1, 1.0));

        println!("Boundaries: {:?}", hist.boundaries);

        hist.increment(0.25);

        hist.print();


    }


}