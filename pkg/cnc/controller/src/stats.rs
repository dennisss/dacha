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

impl MinMaxStats<f64> {
    pub fn print_scaled(&self, scale: f64) -> String {
        format!("[min: {:?}; max: {:?}; range: {:?}]", self.min * scale, self.max * scale, (self.max - self.min) * scale)
    }
}


#[derive(Default)]
pub struct AverageTracker {
    sum: f64,
    count: usize
}

impl AverageTracker {
    pub fn add(&mut self, v: f64) {
        self.count += 1;
        self.sum += v;
    }


    pub fn average(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }

        self.sum / (self.count as f64)
    }

    pub fn count(&self) -> usize {
        self.count
    }
}


pub fn compute_standard_deviation(values: &[f64]) -> f64 {
    let mut average = 0.0;
    for v in values {
        average += *v;
    }
    average /= values.len() as f64;

    let mut out = 0.0;
    for v in values {
        let x = v - average;
        out += x*x;
    }

    (out / (values.len() as f64)).sqrt()
}

// Basically tracks everything
#[derive(Default)]
pub struct NumericalMetricsTracker {
    average: AverageTracker,
    range: MinMaxStats<f64>,
    data: Vec<f64>,
}

impl NumericalMetricsTracker {
    pub fn add(&mut self, v: f64) {
        self.average.add(v);
        self.range.add(v);
        self.data.push(v);
    }

    pub fn mean(&self) -> f64 {
        self.average.average()
    }

    pub fn count(&self) -> usize {
        self.data.len()
    }

    pub fn range(&self) -> &MinMaxStats<f64> {
        &self.range
    }

    pub fn stddev(&self) -> f64 {
        compute_standard_deviation(&self.data[..])
    }
}

