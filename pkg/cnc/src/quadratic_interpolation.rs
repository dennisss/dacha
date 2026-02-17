use alloc::{collections::VecDeque, vec::Vec};

use crate::quadratic_stepper_motion::QuadraticStepperMotion;

/// NOTE: This must be much smaller than half the min step duration to ensure that
/// steps don't swap ordering in time between two consecutive curves.
const MAX_ERROR: i32 = 100;

// TODO: Also read https://klipper.discourse.group/t/improved-stepcompress-implementation/3203


/// Interpolates step times into quadratic stepper motion curves.
///
/// This is done by trying to fit the entire list of step times to a since curve
/// and if that fails we try recursively in each half of the list.
///
/// Running merge_adjacent_motions() after this will usually help a lot on complex
/// motions.
pub fn bisect_fit(step_times: &[u32], out: &mut Vec<QuadraticStepperMotion>) {
    if step_times.len() == 0 {
        return;
    }

    assert!(step_times.len() >= 2);

    let start_time = step_times[0];

    let step_0_end = step_times[1];
    let step_0_duration = step_0_end - start_time;

    if step_times.len() == 2 {
        out.push(QuadraticStepperMotion {
            next_step_time: start_time,
            next_step_duration: step_0_duration,
            step_duration_increment: 0,
            num_steps: 1.into()
        });
        return;
    }

    let step_1_end = step_times[2];
    let step_1_duration = step_1_end - step_0_end;

    // Form an exact curve fit using the two steps
    let mut initial_motion = QuadraticStepperMotion {
        next_step_time: start_time,
        next_step_duration: step_0_duration,
        step_duration_increment: (step_1_duration as i32) - (step_0_duration as i32),
        num_steps: ((step_times.len() - 1) as i32).into()
    };


    let mut current_motion = initial_motion.clone();

    let mut all_good = true;

    // TODO: Don't need to  check first few times.
    for i in 0..(step_times.len() - 1) {
        let error = ((step_times[i] as i32) - (current_motion.next_step_time as i32)).abs();
        if error > MAX_ERROR {
            // println!("Error1: {}", error);
            all_good = false;
            break;
        }

        // TODO: This can be optimized.
        current_motion.next();
    }


    if all_good {
        out.push(initial_motion);
        return;
    }

    // TODO: Check this. Ideally assert the final step counts are right.
    let mid_i = step_times.len() / 2;
    bisect_fit(&step_times[0..(mid_i + 1)], out);
    bisect_fit(&step_times[mid_i..], out);
}

/// Given a list of already interpolated stepper motions for some set of step times,
/// this further compresses the sequence by looking at each motion and trying to steal
/// steps from the previous and next motions. This prioritizes stealing from smaller
/// motions so that we can reduce the otherall count of motions.
///
/// For efficiency, the 'initial_motions' are mutated for intermediate calculations.
pub fn merge_adjacent_motions(
    step_times: &[u32],
    initial_motions: &mut [QuadraticStepperMotion],
    out: &mut Vec<QuadraticStepperMotion>
) {
    assert!(out.is_empty());

    /*
    To find the previous motion, we always look at the end of the 'out' array.
    Forward motions are in initial_motions.
    */

    // Index into 'initial_motions' of the current motion we are looking at.
    let mut motion_i = 0;
    // Index of the first step in the motion at motion_i.
    let mut step_i = 0;

    let mut next_non_empty_motion_i = 0;

    while motion_i < initial_motions.len() {
        let mut cur_motion = initial_motions[motion_i].clone();
        
        if cur_motion.num_steps.count() == 0 {
            motion_i += 1;
            continue;
        }

        // Try to compress with previous motion
        while !out.is_empty() {
            let last_motion = out.last_mut().unwrap();

            if last_motion.num_steps.count() == 0 {
                out.pop();
                continue;
            }

            // Only merge into larger motions.
            if cur_motion.num_steps.count() <= last_motion.num_steps.count() {
                break;
            }

            let prev_step_time = {
                let mut m = cur_motion.clone();
                m.prev();
                m.next_step_time
            };

            let target_step_time = step_times[(step_i - 1) as usize];

            if ((prev_step_time as i32) - (target_step_time as i32)).abs() > MAX_ERROR {
                break;
            }

            cur_motion.prev();
            last_motion.num_steps.dec();
            step_i -= 1;
        }

        // Try to merge with motions ahead of us.
        next_non_empty_motion_i = next_non_empty_motion_i.max(motion_i + 1);
        while next_non_empty_motion_i < initial_motions.len() {
            let next_motion = &mut initial_motions[next_non_empty_motion_i];
            if next_motion.num_steps.count() == 0 {
                next_non_empty_motion_i += 1;
                continue;
            }

            if next_motion.num_steps.count() > cur_motion.num_steps.count() {
                break;
            }

            let next_step_time = cur_motion.step_start_time(cur_motion.num_steps.count() as usize);
            let target_step_time = step_times[(step_i + cur_motion.num_steps.count()) as usize];

            if ((next_step_time as i32) - (target_step_time as i32)).abs() > MAX_ERROR {
                break;
            }

            cur_motion.num_steps.inc();
            next_motion.next();
        }

        motion_i += 1;
        step_i += cur_motion.num_steps.count();
        out.push(cur_motion);
    }

    // println!("Compressed from {} to {} motions", initial_motions.len(), out.len());
}


/// Note that the final time in step_times is the end time of the last step
/// (which starts at time 'step_times[len - 2]')
///
/// NOTE: For simplicy, this assumes step_times[0] is 0 and the
/// total motion length is relatively short so we won't overflow u32.
///
/// TODO: Probably more efficient to have this take as input an iterator.
fn basic_fit(step_times: &[u32], out: &mut Vec<QuadraticStepperMotion>) {
    if step_times.len() == 0 {
        return;
    }
    
    assert!(step_times.len() >= 2);

    let mut start_time = step_times[0];
    let mut i = 1;

    while i < step_times.len() {
        let step_0_end = step_times[i];
        // For this to be true, our error threshold must be much smaller than
        // the min step duration.
        assert!(step_0_end > start_time);
        i += 1;
        let step_0_duration = step_0_end - start_time;

        // Single step curve
        if i == step_times.len() {
            out.push(QuadraticStepperMotion {
                next_step_time: start_time,
                next_step_duration: step_0_duration,
                step_duration_increment: 0,
                num_steps: 1.into()
            });

            start_time = step_0_end;
            break;
        }

        let step_1_end = step_times[i];
        assert!(step_1_end > step_0_end);
        i += 1;
        let step_1_duration = step_1_end - step_0_end;

        // Form an exact curve fit using the two steps
        let mut initial_motion = QuadraticStepperMotion {
            next_step_time: start_time,
            next_step_duration: step_0_duration,
            step_duration_increment: (step_1_duration as i32) - (step_0_duration as i32),
            num_steps: 2.into()
        };

        let mut current_motion = initial_motion.clone();
        // TODO: This can be optimized.

        assert_eq!(current_motion.next_step_time, start_time);
        current_motion.next();
        assert_eq!(current_motion.next_step_time, step_0_end);
        current_motion.next();
        assert_eq!(current_motion.next_step_time, step_1_end);
        start_time = step_1_end;


        // Look at future steps while they fit the current motion.
        while i < step_times.len() {
            let step_i_end = step_times[i];
            assert!(step_i_end > current_motion.next_step_time);

            // Check what time the step would end at with the current motion.
            current_motion.num_steps.inc();
            current_motion.next();
            let proposed_step_i_end = current_motion.next_step_time;

            let error = ((step_i_end as i32) - (proposed_step_i_end as i32)).abs();
            println!("Error: {}", error);
            if error > MAX_ERROR {
                break;
            }

            // Consume the step.
            i += 1;
            // TODO: This can be optimed out probably.
            initial_motion.num_steps.inc();
            // The 'max' here allows the next curve to start with a delay after the current
            // one to reduce the overall error.
            // start_time = core::cmp::max(step_i_end, proposed_step_i_end);
            start_time = proposed_step_i_end;
        }


        out.push(initial_motion);
    }
}