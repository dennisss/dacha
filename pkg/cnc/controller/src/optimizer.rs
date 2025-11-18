use std::ops::{Index, IndexMut};
use std::fmt::Debug;
use std::time::Instant;

use common::errors::*;
use executor::channel;

pub trait OptimizerInput {
    fn weight(&self, i: usize) -> f32;

    /// NOTE: This should not perform any clamping so that we can accurately calculate gradients. 
    fn set_weight(&mut self, i: usize, v: f32);

    fn clamp_weight(&self, i: usize, v: f32) -> f32;

    fn weights_len(&self) -> usize;

    fn debug_weights(&self) -> String;

    fn learning_rate(&self) -> f32;

    /// A step of 'None' means that it is just usedfor internal tuning of weights.
    fn calculate_error(&mut self, step: Option<usize>) -> f32;
}

#[derive(Defaultable)]
pub struct OptimizerOptions {
    #[default(10)]
    pub monitor_steps_interval: usize,

    #[default(true)]
    pub print_progress: bool,

    #[default(Some(0.0001))]
    pub min_error_improvement_fraction: Option<f32>,

    pub max_steps: Option<usize>
}

pub async fn gradient_descent(
    input: &mut OptimizerInput, options: OptimizerOptions,
) -> Result<()> {
    let mut training_step = 0;
    let mut last_error = 0.0;
    let mut last_time = Instant::now();

    loop {

        // TODO: Need some detection of oscillating error or exploding error and maybe compare to the
        // min error across all steps for progress checking.
        if training_step % options.monitor_steps_interval == 0 {
            let error = input.calculate_error(Some(training_step));


            let t = Instant::now();

            let time_per_step = (t - last_time).as_secs_f32() / (options.monitor_steps_interval as f32);

            last_time = t;

            if options.print_progress {
                println!("[Step: {}] [Weights: {}] [Error: {}] [Steps/s: {:.2?}]",
                    training_step, input.debug_weights(), error, 1.0 / time_per_step);
            }

            if let Some(threshold) = options.min_error_improvement_fraction {
                if training_step > 0 && ((last_error - error) / last_error).abs() < threshold {
                    break;
                }
            }

            last_error = error;
        }

        let mut de_dw = vec![0.0; input.weights_len()];

        for i in 0..input.weights_len() {
            let original_weight = input.weight(i);

            input.set_weight(i, original_weight + 0.001);
            let e_a = input.calculate_error(None);

            input.set_weight(i, original_weight - 0.001);
            let e_b = input.calculate_error(None);

            de_dw[i] = (e_a - e_b) / 0.002;

            // Restore the value.
            input.set_weight(i, original_weight);
        }

        let learning_rate = input.learning_rate();

        for i in 0..input.weights_len() {
            let mut w = input.weight(i); 
            w += learning_rate * -de_dw[i];
            w = input.clamp_weight(i, w);

            input.set_weight(i, w);
        }

        training_step += 1;

        if let Some(max_steps) = options.max_steps {
            if training_step > max_steps {
                break;
            }
        }
    }

    Ok(())
}