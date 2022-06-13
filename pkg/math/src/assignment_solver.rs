use alloc::vec::Vec;

use typenum::U2;

use crate::matrix::*;
use crate::number::Zero;

#[derive(Clone, Copy, PartialEq)]
enum Entry {
    Empty,
    Star,
    Prime,
}

impl Default for Entry {
    fn default() -> Self {
        Self::Empty
    }
}

impl Zero for Entry {
    fn zero() -> Self {
        Entry::Empty
    }
}

#[derive(Clone, Copy)]
enum Step {
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    Done,
}

/// Solves the optimal assignment problem.
/// Internally this uses the Munkres (aka Hungarian) Algorithm
///
/// See http://csclab.murraystate.edu/~bob.pilgrim/445/munkres.html for a great
/// reference.
pub struct AssignmentSolver {
    epsilon: f64,
    W: MatrixXd,                        // The square working matrix
    M: Matrix<Entry, Dynamic, Dynamic>, // Current entry marking

    row_lines: Vec<bool>,
    col_lines: Vec<bool>,

    path: Vec<Vector<usize, U2>>,
}

impl AssignmentSolver {
    pub fn new() -> AssignmentSolver {
        Self::new_with_epsilon(1e-5)
    }

    // Create a new solver instance
    //
    // @param epsilon the largest floating point value that should be considered
    // zero
    pub fn new_with_epsilon(epsilon: f64) -> AssignmentSolver {
        AssignmentSolver {
            epsilon,
            W: Matrix::zero_with_shape(0, 0),
            M: Matrix::zero_with_shape(0, 0),
            row_lines: vec![],
            col_lines: vec![],
            path: vec![],
        }
    }

    // Does the solving
    //
    // @param w the weights associated with assignment. This can be of any size N x
    // M @param c the output assignments of each row index. -1 if the row could
    // not be assigned @return the total cost of the found assignment
    pub fn solve(&mut self, w: &MatrixXd, c: &mut Vec<Option<usize>>) -> f64 {
        // Padding with zeros to be a square matrix with the same optimal assignments
        let N = core::cmp::max(w.rows(), w.cols());
        self.W = MatrixXd::zero_with_shape(N, N); // < TODO: Instead resize and clear?
        self.W
            .block_with_shape_mut(0, 0, w.rows(), w.cols())
            .copy_from(w);

        self.M = Matrix::zero_with_shape(self.W.rows(), self.W.cols());

        self.row_lines.clear();
        self.row_lines.resize(self.W.rows(), false);
        self.col_lines.clear();
        self.col_lines.resize(self.W.cols(), false);

        // Do the solving
        let mut step = Step::S1;
        let mut it = 0;
        while it < self.W.rows() * 7 {
            step = match step {
                Step::S1 => self.solve_step1(),
                Step::S2 => self.solve_step2(),
                Step::S3 => self.solve_step3(),
                Step::S4 => self.solve_step4(),
                Step::S5 => self.solve_step5(),
                Step::S6 => self.solve_step6(),
                Step::Done => break,
            };

            it += 1;
        }

        // Extract final assignment
        let mut cost = 0.0;
        c.resize(w.rows(), None);
        for i in 0..w.rows() {
            let j = self.find_in_row(Entry::Star, i).unwrap();
            if j < w.cols() {
                c[i] = Some(j);
            } else {
                c[i] = None;
            }

            cost += w[(i, j)];
        }

        cost
    }

    #[inline]
    fn is_zero(&self, i: usize, j: usize) -> bool {
        self.W[(i, j)].abs() < self.epsilon
    }

    #[inline]
    fn covered(&self, i: usize, j: usize) -> bool {
        self.row_lines[i] || self.col_lines[j]
    }

    fn solve_step1(&mut self) -> Step {
        // Subtract row mins
        for i in 0..self.W.rows() {
            // Find min
            let mut min = core::f64::MAX;
            for j in 0..self.W.cols() {
                if self.W[(i, j)] < min {
                    min = self.W[(i, j)];
                }
            }

            // Subtract min
            for j in 0..self.W.cols() {
                self.W[(i, j)] -= min;
            }
        }

        Step::S2
    }

    fn solve_step2(&mut self) -> Step {
        // Star all zeros not already in a row/column with a stared value
        for i in 0..self.W.rows() {
            for j in 0..self.W.cols() {
                if self.is_zero(i, j) && !self.covered(i, j) {
                    self.M[(i, j)] = Entry::Star;
                    self.row_lines[i] = true;
                    self.col_lines[j] = true;
                }
            }
        }

        self.reset_cover();
        Step::S3
    }

    fn solve_step3(&mut self) -> Step {
        // Check if we've covered everything
        for i in 0..self.W.rows() {
            for j in 0..self.W.cols() {
                if self.M[(i, j)] == Entry::Star {
                    self.col_lines[j] = true;
                }
            }
        }

        let mut n = 0;
        for i in 0..self.col_lines.len() {
            if self.col_lines[i] {
                n += 1;
            }
        }

        if n == self.M.cols() {
            Step::Done
        } else {
            Step::S4
        }
    }

    fn solve_step4(&mut self) -> Step {
        let mut i = 0;
        let mut j = 0;

        loop {
            if let Some((i, j)) = self.find_uncovered_zero() {
                self.M[(i, j)] = Entry::Prime;

                if let Some(j) = self.find_in_row(Entry::Star, i) {
                    self.row_lines[i] = true;
                    self.col_lines[j] = false;
                } else {
                    // Save for the next step
                    self.path.clear();
                    self.path.push(Vector::from_slice(&[i, j]));
                    return Step::S5;
                }
            } else {
                return Step::S6;
            }
        }
    }

    fn solve_step5(&mut self) -> Step {
        // Expanding the path created in the last step
        loop {
            let j = self.path[self.path.len() - 1][1];

            let i = match self.find_in_col(Entry::Star, j) {
                Some(i) => i,
                None => break,
            };

            self.path.push(Vector::from_slice(&[i, j]));

            let j = self.find_in_row(Entry::Prime, i).unwrap();
            self.path.push(Vector::from_slice(&[i, j]));
        }

        self.augment_path();
        self.reset_cover();
        self.reset_primes();
        Step::S3
    }

    fn solve_step6(&mut self) -> Step {
        // Find smallest uncovered value.
        let mut min = core::f64::MAX;

        for i in 0..self.W.rows() {
            for j in 0..self.W.cols() {
                if !self.covered(i, j) && self.W[(i, j)] < min {
                    min = self.W[(i, j)];
                }
            }
        }

        for i in 0..self.W.rows() {
            for j in 0..self.W.cols() {
                if self.row_lines[i] {
                    self.W[(i, j)] += min;
                }
                if !self.col_lines[j] {
                    self.W[(i, j)] -= min;
                }
            }
        }

        Step::S4
    }

    fn find_uncovered_zero(&self) -> Option<(usize, usize)> {
        for i in 0..self.W.rows() {
            for j in 0..self.W.cols() {
                if self.is_zero(i, j) && !self.covered(i, j) {
                    return Some((i, j));
                }
            }
        }

        None
    }

    // Returns the column index.
    fn find_in_row(&mut self, entry: Entry, i: usize) -> Option<usize> {
        for j in 0..self.M.cols() {
            if self.M[(i, j)] == entry {
                return Some(j);
            }
        }

        None
    }

    // Returns the row index
    fn find_in_col(&self, entry: Entry, j: usize) -> Option<usize> {
        for i in 0..self.M.rows() {
            if self.M[(i, j)] == entry {
                return Some(i);
            }
        }

        None
    }

    fn augment_path(&mut self) {
        let mut i = 0;
        let mut j = 0;
        for p in 0..self.path.len() {
            i = self.path[p][0];
            j = self.path[p][1];

            self.M[(i, j)] = if self.M[(i, j)] == Entry::Star {
                Entry::Empty
            } else {
                Entry::Star
            };
        }
    }

    fn reset_cover(&mut self) {
        // Reset lines
        for i in 0..self.W.rows() {
            self.row_lines[i] = false;
            self.col_lines[i] = false;
        }
    }

    fn reset_primes(&mut self) {
        for i in 0..self.M.rows() {
            for j in 0..self.M.cols() {
                if self.M[(i, j)] == Entry::Prime {
                    self.M[(i, j)] = Entry::Empty;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn works() {
        let mut solver = AssignmentSolver::new();

        let mut c = Vec::new();
        let mut cost;

        let mut A =
            MatrixXd::from_slice_with_shape(3, 3, &[2.0, 3.0, 3.0, 3.0, 2.0, 3.0, 3.0, 3.0, 2.0]);

        cost = solver.solve(&A, &mut c);
        assert_eq!(cost, 6.0);
        assert_eq!(c[0], Some(0));
        assert_eq!(c[1], Some(1));
        assert_eq!(c[2], Some(2));

        ////

        A.copy_from_slice(&[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);

        cost = solver.solve(&A, &mut c);
        assert_eq!(cost, 3.0);

        ////

        A.copy_from_slice(&[1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 3.0, 6.0, 9.0]);

        cost = solver.solve(&A, &mut c);
        assert_eq!(cost, 10.0);

        ////

        let A = MatrixXd::from_slice_with_shape(
            4,
            4,
            &[
                1.0, 2.0, 3.0, 4.0, 2.0, 4.0, 6.0, 8.0, 3.0, 6.0, 9.0, 12.0, 4.0, 8.0, 12.0, 16.0,
            ],
        );

        cost = solver.solve(&A, &mut c);
        assert_eq!(cost, 20.0);
    }
}
