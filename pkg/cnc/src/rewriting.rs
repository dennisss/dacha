use alloc::vec::Vec;
use math::matrix::VectorXd;

use crate::grid::interp;

pub struct RewriteMoveOptions {
    pub min_move_size: f64,
    pub step_size: f64,
    pub max_error: f64,
}

/// Returns the list of all points to visit AFTER start_position.
/// (will just be a list with 1 element containing just end_position is
///  no Z compensation or very little is needed)
pub fn rewrite_move_z<F: Fn(&VectorXd) -> f64>(
    start_position: &VectorXd,
    end_position: &VectorXd,
    rapid: bool,
    new_z: F,
    options: &RewriteMoveOptions,
) -> Vec<VectorXd> {
    let mut dir = end_position - start_position;
    for i in 2..dir.len() {
        dir[i] = 0.0;
    }
    
    let distance = dir.norm();
    dir.normalize();

    let mut out = vec![];

    // TODO: Need to also apply to rapid unless we are high enough.

    if distance > options.min_move_size && !rapid {
        
        let step_size = options.step_size;
        // let step_size = self.x_interval.min(self.y_interval) / 2.0;

        // TODO: Ideally make this smarter and snap to the grid boundaries
        let mut t = step_size;
        while t < distance - options.min_move_size {
            let mut pt = start_position + (dir.clone() * t);

            for i in 2..dir.len() {
                pt[i] = interp(end_position[i], start_position[i], t / distance);
            }

            pt[2] = new_z(&pt);

            out.push(pt);

            t += step_size;
        }
    }

    // Add the final point.
    {
        let mut end_position = end_position.clone();
        end_position[2] = new_z(&end_position);
        out.push(end_position);
    }

    // Merge the points into larger lines as long as the error stays under a threshold.
    let mut out_merged = vec![];
    {
        let mut last_point = start_position;

        // Index of the next non-processed point in 'out'
        // (first point we want to move to after last_point).
        let mut i = 0;

        while i < out.len() {
            // The current value of 'j' represents that we can collapse all
            // points at indexes out[i..j]
            //
            // (This default value implies just merging a single point)
            let mut j = i + 1;

            while j < out.len() {
                // Propose to form a line with one more point.
                let end_point = &out[j];

                let dir = (end_point - last_point).normalized();

                let mut good = true;

                // Check all intermediate points.
                for j in i..j {
                    let mut pt_dir = &out[i] - last_point;
                    pt_dir -= dir.clone() * pt_dir.dot(&dir);
                    let error = pt_dir.norm();

                    if error > options.max_error {
                        good = false;
                        break;
                    }
                }

                if good {
                    j += 1;
                } else {
                    break;
                }
            }

            out_merged.push(out[j - 1].clone());
            last_point = &out[j - 1];
            i = j;
        }
    }

    out_merged
}