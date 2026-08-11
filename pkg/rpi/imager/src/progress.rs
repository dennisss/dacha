use std::time::Instant;

use base_units::ByteCount;

pub struct ProgressTracker {
    start_time: Instant,
    total_bytes: usize,

    last_time: Instant,
    last_percentage: usize,
    last_written_bytes: usize,
}

impl ProgressTracker {
    pub fn new(total_bytes: usize) -> Self {
        let t = Instant::now();
        Self {
            start_time: t.clone(),
            total_bytes,

            last_time: t.clone(),
            last_percentage: 0,
            last_written_bytes: 0,
        }
    }

    pub fn update(&mut self, written_bytes: usize) {
        let percent = (100 * written_bytes) / self.total_bytes;
        if percent == self.last_percentage {
            return;
        }

        let time = Instant::now();

        let rate = ((written_bytes - self.last_written_bytes) as f64)
            / (time - self.last_time).as_secs_f64();
        println!("=> {}% [{:?}/s]", percent, ByteCount::from(rate as usize));

        if percent == 100 {
            println!("Done! Took: {:?}", time - self.start_time);
        }

        self.last_percentage = percent;
        self.last_written_bytes = written_bytes;
        self.last_time = time;
    }
}