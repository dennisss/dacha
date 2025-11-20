use core::fmt::Debug;

#[cfg(feature = "alloc")]
use alloc::{collections::VecDeque, vec::Vec};

#[cfg(feature = "alloc")]
use crate::quadratic_interpolation::*;


/// Stepper motor motion curve which follows a quadratic position curve
/// as a function of time.
#[derive(Debug, Clone)]
pub struct QuadraticStepperMotion {
    /// Time at which the next step should start.
    /// (before any steps are run, this will also be the time of the first step).
    pub next_step_time: u32,
    
    /// How long the next step should run for before the step after it should be
    /// triggered.
    pub next_step_duration: u32,

    /// How much to increase/decrease the next_step_duration after each step.
    pub step_duration_increment: i32,
    
    /// Total number of steps we need to run.
    /// This also encodes the direction of the steps.
    pub num_steps: StepCount,
}

impl QuadraticStepperMotion {
    pub fn next(&mut self) {
        self.next_step_time = self.next_step_time.wrapping_add(self.next_step_duration);
        self.next_step_duration =
            self.next_step_duration.wrapping_add(self.step_duration_increment as u32);
        self.num_steps.dec();
    }

    pub fn prev(&mut self) {
        self.num_steps.inc();
        self.next_step_duration =
            self.next_step_duration.wrapping_sub(self.step_duration_increment as u32);
        self.next_step_time = self.next_step_time.wrapping_sub(self.next_step_duration);
    }

    /// Gets the starting time for step i (where the first step is i=0).
    /// TODO: Test this.
    pub fn step_start_time(&self, i: usize) -> u32 {
        // t_i = t_0 + i * duration_0 + (n*(n-1))*increment

        let t: u32 = self.next_step_time.wrapping_add((i as u32) * self.next_step_duration);
        
        ((t as i32) + (sum_1_to_n(i - 1) as i32) * self.step_duration_increment) as u32
    }

    #[cfg(feature = "alloc")]
    pub fn interpolate_step_times(step_times: &[u32], out: &mut Vec<Self>) {
        let mut tmp = vec![];
        bisect_fit(step_times, &mut tmp);
        merge_adjacent_motions(step_times, &mut tmp, out);

        // TODO: Sanity check that we still have the same number of steps in the outptu.
    }
}

fn sum_1_to_n(n: usize) -> usize {
    n * (n + 1) / 2
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct StepCount {
    value: u32
}

impl From<i32> for StepCount {
    fn from(v: i32) -> Self {
        Self::new(v.abs() as u32, v >= 0)
    }
}

impl StepCount {
    pub fn new(count: u32, dir: bool) -> Self {
        let mut v = count & ((1 << 31) - 1);
        if dir {
            v |= 1 << 31;
        }
        Self { value: v }
    }

    pub fn count(&self) -> u32 {
        self.value & ((1 << 31) - 1)
    }

    pub fn direction(&self) -> bool {
        self.sign_bit() != 0
    }

    pub fn delta(&self) -> i32 {
        let mut count = self.count() as i32;
        if !self.direction() {
            count *= -1;
        }
        count
    }

    fn sign_bit(&self) -> u32 {
        self.value & (1 << 31)
    }

    pub fn dec(&mut self) {
        // TODO: If we don't care about bounds checks, we can
        // replace this with 'self.value -= 1'.
        self.value = (self.count() - 1) | self.sign_bit();
    }

    pub fn inc(&mut self) {
        self.value = (self.count() + 1) | self.sign_bit();
    }
}


impl core::fmt::Debug for StepCount {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.delta())
    }
}