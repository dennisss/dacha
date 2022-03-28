use typenum::U1;

use crate::matrix::base::VectorNew;
use crate::matrix::dimension::Dimension;
use crate::matrix::storage::{MatrixNewStorage, NewStorage};

/// Axis aligned bounding box.
pub struct BoundingBox<D: Dimension>
where
    MatrixNewStorage: NewStorage<f32, D, U1>,
{
    pub min: VectorNew<f32, D>,
    pub max: VectorNew<f32, D>,
}

impl<D: Dimension> BoundingBox<D>
where
    MatrixNewStorage: NewStorage<f32, D, U1>,
{
    pub fn compute(points: &[VectorNew<f32, D>]) -> Self {
        if points.len() == 0 {
            return Self {
                min: VectorNew::null(),
                max: VectorNew::null(),
            };
        }

        let mut min = points[0].clone();
        let mut max = points[0].clone();

        for p in &points[1..] {
            for i in 0..p.len() {
                min[i] = f32::min(min[i], p[i]);
                max[i] = f32::max(max[i], p[i]);
            }
        }

        Self { min, max }
    }

    pub fn clip(mut self, clipbox: &BoundingBox<D>) -> Self {
        for i in 0..self.min.len() {
            self.min[i] = f32::max(self.min[i], clipbox.min[i]);
            self.max[i] = f32::min(self.max[i], clipbox.max[i]);
        }

        self
    }
}
