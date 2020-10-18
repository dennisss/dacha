use std::time::{Duration, Instant};

pub const CYCLES_PER_SECOND: u64 = 1 << 22; // 4MHz
pub const NANOS_PER_SECOND: u64 = 1000000000;

/// The clock is a counter that runs at 4 * 1,048,576 Hz.
pub struct Clock {
    start_time: Instant,
    start_cycles: u64,
    pub cycles: u64,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            start_cycles: 0,
            cycles: 0,
        }
    }

    pub fn reset_start(&mut self) {
        self.start_time = Instant::now();
        self.start_cycles = self.cycles;
    }

    pub fn now(&self) -> Timestamp {
        let elapsed = Duration::from_nanos(
            ((self.cycles - self.start_cycles) * NANOS_PER_SECOND) / CYCLES_PER_SECOND,
        );

        Timestamp {
            cycles: self.cycles,
            current_time: self.start_time + elapsed,
        }
    }

    pub fn target(&self) -> u64 {
        let elapsed = Instant::now() - self.start_time;
        let ncycles = (elapsed.as_secs_f64() * (CYCLES_PER_SECOND as f64)) as u64;

        ncycles + self.start_cycles
    }
}

pub struct Timestamp {
    current_time: Instant,
    cycles: u64,
}

impl Timestamp {
    pub fn cycles_1mhz(&self) -> u64 {
        self.cycles / 4
    }
    pub fn cycles_512hz(&self) -> u64 {
        self.cycles / 8192
    }
}
