// Utilities for creating test cases.

use std::time::{Duration, Instant};

use crate::random::Rng;

pub struct TimingLeakTest<I, F> {
    test_case_generator: I,
    test_case_runner: F,
    options: TimingLeakTestOptions,
}

pub struct TimingLeakTestOptions {
    pub num_iterations: usize,
}

impl<T, I: Iterator<Item = T>, F: Fn(&T) -> bool> TimingLeakTest<I, F> {
    pub fn new(
        test_case_generator: I,
        test_case_runner: F,
        options: TimingLeakTestOptions,
    ) -> Self {
        Self {
            test_case_generator,
            test_case_runner,
            options,
        }
    }

    pub fn run(&mut self) {
        let mut test_case_index = 0;
        let mut min_time = Duration::MAX;
        let mut max_time = Duration::ZERO;
        while let Some(test_case) = self.test_case_generator.next() {
            let start = Instant::now();
            for _ in 0..self.options.num_iterations {
                self.runner_wrap(&test_case);
            }
            let end = Instant::now();

            let duration = end.duration_since(start);

            min_time = std::cmp::min(min_time, duration);
            max_time = std::cmp::max(max_time, duration);

            println!("Test case {}: {:?}", test_case_index, duration);

            test_case_index += 1;
        }

        let min = min_time.as_secs_f64();
        let max = max_time.as_secs_f64();

        // < 1% deviation between runs.
        // assert!(
        //     max - min < max * 0.01,
        //     "{:?} < {:?}",
        //     Duration::from_secs_f64(max - min),
        //     Duration::from_secs_f64(max * 0.01)
        // );
    }

    #[inline(never)]
    fn runner_wrap(&self, test_case: &T) -> bool {
        (self.test_case_runner)(test_case)
    }
}

pub fn typical_boundary_buffers(length: usize) -> Vec<Vec<u8>> {
    let mut out = vec![];

    // Zero buffer.
    out.push(vec![0u8; length]);

    // 0xFF buffer.
    out.push(vec![0xFFu8; length]);

    // First byte set to value #1
    out.push({
        let mut v = vec![0u8; length];
        v[0] = 0xAB;
        v
    });
    // Last byte set to value #2
    out.push({
        let mut v = vec![0u8; length];
        v[0] = 0xCD;
        v
    });

    if length > 1 {
        // Last byte set to value #1
        out.push({
            let mut v = vec![0u8; length];
            *v.last_mut().unwrap() = 0x20;
            v
        });
        // Last byte set to value #2
        out.push({
            let mut v = vec![0u8; length];
            *v.last_mut().unwrap() = 0x03;
            v
        });
    }

    if length > 2 {
        // Even bytes set.
        out.push({
            let mut v = vec![0u8; length];
            for i in (0..v.len()).step_by(2) {
                v[i] = i as u8
            }
            v
        });

        // Odd bytes set
        out.push({
            let mut v = vec![0u8; length];
            for i in (1..v.len()).step_by(2) {
                v[i] = i as u8
            }
            v
        });

        let mid_idx = length / 2;

        // First half set
        out.push({
            let mut v = vec![0u8; length];
            for i in 0..mid_idx {
                v[i] = i as u8
            }
            v
        });

        // Second half set
        out.push({
            let mut v = vec![0u8; length];
            for i in mid_idx..length {
                v[i] = i as u8
            }
            v
        });
    }

    let mut rng = crate::random::MersenneTwisterRng::mt19937();
    rng.seed_u32(1234);

    // A few random buffers.
    for _ in 0..3 {
        out.push({
            let mut v = vec![0u8; length];
            rng.generate_bytes(&mut v);
            v
        });
    }

    out
}

// TODO: Measure things in clock cycles to avoid float division at the time of
// the test.
// (but will need to fix the clock frequency).

// 10 seconds in nanos: 10000000000

pub struct RollingMean {
    sum: u64,
    n: usize,
}

// impl RollingMean {
//     pub fn add

// }
