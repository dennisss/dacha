// Utilities for creating test cases.

use core::marker::PhantomData;
use std::time::{Duration, Instant};
use std::vec::Vec;

use common::errors::*;

use crypto::random::Rng;

/*
To verify no timing leaks:
- Run any many different input sizes.
- Verify compiler optimizations aren't making looping over many iterations too fast
    - Speed is linear w.r.t. num iterations.
    - We can even experimentally determine if algorithms are linear, quadratic, etc.
- Compare average time between trials and make sure variance is lower than some constant.

TODO: Add test cases to verify that this can fail sometimes.
*/

#[derive(Default)]
pub struct TimingLeakTestCase<T> {
    pub data: T,
    pub size: usize,
}

/// Creates new test cases for the timing test.
pub trait TimingLeakTestCaseGenerator {
    type Data;

    /// Should generate a new test case and write it into 'out'.
    /// - If any heap memory is involved, the new test case should re-use as
    ///   much of the memory buffers already in 'out' as possible to avoid any
    ///   memory location/cache related noise.
    ///
    /// Returns true if a new test case was generated or false if we ran out of
    /// test cases.
    fn next_test_case(&mut self, out: &mut TimingLeakTestCase<Self::Data>) -> bool;
}

/// Function which runs some code being profiled once in the calling thread.
///
/// To avoid computations being pruned, this function should return the final
/// 'result' of the computation or some value that can only be determined after
/// the computation is done.
pub trait TimingLeakTestCaseRunner<T, R> = Fn(&T) -> Result<R>;

/// Test wrapper for executing some caller provided code to ensure that it
/// 'probably' executes in constant time regardless of which values are passed
/// to it.
///
/// We expect that the code should vary in execution time with the size of the
/// input data (number of bytes processed) but not with the contents of those
/// bytes.
///
/// This performs testing purely through black box methods so we can't necessary
/// fully gurantee that cache misses or branch mispredictions won't compromise
/// the security of the function.
///
/// The caller provides two things:
/// 1. A TimingLeakTestCaseGenerator which should create a wide range of inputs
/// to the code under test. There should be sufficient test cases to attempt to
/// make the code under test perform very quickly or slowly.
///
/// 2. A TimingLeakTestCaseRunner which takes a test case as input and runs the
/// code being profiled.
///
/// NOTE: This can not track any work performed on other threads.
pub struct TimingLeakTest<R, Gen, Runner> {
    test_case_generator: Gen,
    test_case_runner: Runner,
    options: TimingLeakTestOptions,
    r: PhantomData<R>,
}

pub struct TimingLeakTestOptions {
    /// Number of separate sets of iterations we will run run the test case
    /// runner for a single test case.
    ///
    /// This must be a value that is at least 1. Setting a value > 1 will
    /// 'measure' a single test case multiple times and will discard rounds that
    /// are outliers (currently only the fasest round is considered).
    pub num_rounds: usize,

    /// Number of times the test case runner will be executed in a single round.
    ///
    /// The total number of times it will be run is 'num_test_cases * num_rounds
    /// * num_iterations'.
    ///
    /// TODO: Automatically figure this out based on a target run time.
    pub num_iterations: usize,
}

impl TimingLeakTest<(), (), ()> {
    pub fn new_generator() -> TimingLeakTestBinaryGenericTestCaseGenerator {
        TimingLeakTestBinaryGenericTestCaseGenerator::default()
    }
}

impl<R, Gen: TimingLeakTestCaseGenerator, Runner: TimingLeakTestCaseRunner<Gen::Data, R>>
    TimingLeakTest<R, Gen, Runner>
where
    Gen::Data: Default,
{
    pub fn new(
        test_case_generator: Gen,
        test_case_runner: Runner,
        options: TimingLeakTestOptions,
    ) -> Self {
        Self {
            test_case_generator,
            test_case_runner,
            options,
            r: PhantomData,
        }
    }

    #[must_use]
    pub fn run(&mut self) -> Result<()> {
        let mut cycle_tracker = perf::CPUCycleTracker::create()?;

        // Check how long it takes for us to just get the number of cycles executed.
        let cycles_noise_floor = {
            let mut floor = 0;

            let mut last_cycles = cycle_tracker.total_cycles()?;
            for _ in 0..10 {
                let next_cycles = cycle_tracker.total_cycles()?;
                floor = core::cmp::max(floor, (next_cycles - last_cycles));
                last_cycles = next_cycles;
            }

            floor
        };

        let mut test_case_index = 0;

        let mut time_stats = StatisticsTracker::new();
        let mut cycle_stats = StatisticsTracker::new();

        let mut test_case = TimingLeakTestCase::default();

        while self.test_case_generator.next_test_case(&mut test_case) {
            let mut case_time_stats = StatisticsTracker::new();
            let mut case_cycle_stats = StatisticsTracker::new();

            for _ in 0..self.options.num_rounds {
                let start = Instant::now();
                let start_cycles = cycle_tracker.total_cycles()?;
                for _ in 0..self.options.num_iterations {
                    self.runner_wrap(&test_case.data);
                }
                let end_cycles = cycle_tracker.total_cycles()?;
                let end = Instant::now();

                let duration = end.duration_since(start);
                let cycle_duration = end_cycles - start_cycles;

                case_time_stats.update(duration);
                case_cycle_stats.update(cycle_duration);

                if cycle_duration < 100 * cycles_noise_floor {
                    return Err(format_err!(
                        "Cycle duration of {} too small relative to noise floor {}",
                        cycle_duration,
                        cycles_noise_floor
                    ));
                }

                // If this is true, then most likely the test code was optimized out by the
                // compiler.
                if duration < Duration::from_millis(2) {
                    return Err(format_err!(
                        "Extremely short round execution time: {:?}",
                        duration
                    ));
                }

                // println!(
                //     "Test case {}: {:?} : {}",
                //     test_case_index, duration, cycle_duration
                // );
            }

            time_stats.update(case_time_stats.min.unwrap());
            cycle_stats.update(case_cycle_stats.min.unwrap());

            test_case_index += 1;
        }

        // TODO: Check that the min cycles time is much larger than the

        let cycle_range = {
            ((cycle_stats.max.unwrap() - cycle_stats.min.unwrap()) as f64)
                / (cycle_stats.min.unwrap() as f64)
                * 100.0
        };

        let time_range = {
            let min = time_stats.min.unwrap().as_secs_f64();
            let max = time_stats.max.unwrap().as_secs_f64();

            (max - min) / min * 100.0
        };

        println!(
            "- Fastest round: {:?} ({} cycles)",
            time_stats.min.unwrap(),
            cycle_stats.min.unwrap()
        );
        println!(
            "- Fastest iteration: {:?}",
            time_stats.min.unwrap() / (self.options.num_iterations as u32)
        );
        println!("- Cycles range: {:0.2}%", cycle_range);
        println!("- Time range: {:0.2}%", time_range);

        // Must have < 1% deviation across different test inputs.
        if cycle_range > 1.0 {
            return Err(format_err!(
                "Cycle range between test cases too large: {:0.2}% > 1%",
                cycle_range
            ));
        }

        Ok(())
    }

    /// Wrapper around the runner which can't be inlined to prevent
    /// optimizations.
    #[inline(never)]
    fn runner_wrap(&self, test_case: &Gen::Data) -> Result<R> {
        (self.test_case_runner)(test_case)
    }
}

#[derive(Default)]
pub struct TimingLeakTestGenericTestCase {
    inputs: Vec<Vec<u8>>,
}

impl TimingLeakTestGenericTestCase {
    pub fn get_input(&self, idx: usize) -> &[u8] {
        &self.inputs[idx]
    }
}

#[derive(Default)]
pub struct TimingLeakTestBinaryGenericTestCaseGenerator {
    inputs: Vec<Vec<Vec<u8>>>,
    last_position: Option<Vec<usize>>,
}

impl TimingLeakTestBinaryGenericTestCaseGenerator {
    pub fn add_input(&mut self, values: Vec<Vec<u8>>) -> usize {
        let idx = self.inputs.len();
        self.inputs.push(values);
        idx
    }

    fn next_position(&self) -> Option<Vec<usize>> {
        for i in &self.inputs {
            assert!(i.len() > 0);
        }

        let mut cur = match self.last_position.clone() {
            Some(v) => v,
            None => return Some(vec![0; self.inputs.len()]),
        };

        for i in (0..cur.len()).rev() {
            cur[i] += 1;
            if cur[i] == self.inputs[i].len() {
                cur[i] = 0;
            } else {
                return Some(cur);
            }
        }

        None
    }
}

impl TimingLeakTestCaseGenerator for TimingLeakTestBinaryGenericTestCaseGenerator {
    type Data = TimingLeakTestGenericTestCase;

    fn next_test_case(
        &mut self,
        out: &mut TimingLeakTestCase<TimingLeakTestGenericTestCase>,
    ) -> bool {
        let pos = match self.next_position() {
            Some(v) => v,
            None => return false,
        };

        out.data.inputs.resize(pos.len(), vec![]);

        for i in 0..pos.len() {
            out.data.inputs[i].clear();
            out.data.inputs[i].extend_from_slice(&self.inputs[i][pos[i]]);
        }

        self.last_position = Some(pos);

        true
    }
}

/// Generates synthetic data buffers whichare likely to trigger different edge
/// cases and time complexities in code that is sensitive (in terms of # of bits
/// set, magnitude, ...) to the value of the data passed it.
pub fn typical_boundary_buffers(length: usize) -> Vec<Vec<u8>> {
    let mut out = vec![];

    // All zeros.
    out.push(vec![0u8; length]);

    // All 0xFF.
    out.push(vec![0xFFu8; length]);

    // First byte set to value #1
    out.push({
        let mut v = vec![0u8; length];
        v[0] = 0xAB;
        v
    });
    // First byte set to value #2
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

pub struct StatisticsTracker<T> {
    min: Option<T>,
    max: Option<T>,
}

impl<T: Ord + Copy> StatisticsTracker<T> {
    pub fn new() -> Self {
        Self {
            min: None,
            max: None,
        }
    }

    pub fn update(&mut self, value: T) {
        self.min = Some(match self.min {
            Some(v) => core::cmp::min(v, value),
            None => value,
        });

        self.max = Some(match self.max {
            Some(v) => core::cmp::max(v, value),
            None => value,
        });
    }
}

pub struct RollingMean {
    sum: u64,
    n: usize,
}
