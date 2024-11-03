use typenum::U1;

use crate::matrix::base::VectorNew;
use crate::matrix::dimension::Dimension;
use crate::matrix::storage::{MatrixNewStorage, NewStorage};

pub struct BoundingBoxBuilder<D: Dimension>
where
    MatrixNewStorage: NewStorage<f32, D, U1>,
{
    min_max: Option<(VectorNew<f32, D>, VectorNew<f32, D>)>,
}

impl<D: Dimension> BoundingBoxBuilder<D>
where
    MatrixNewStorage: NewStorage<f32, D, U1>,
{
    pub fn new() -> Self {
        Self { min_max: None }
    }

    pub fn update(&mut self, point: &VectorNew<f32, D>) {
        let (min, max) = match self.min_max.as_mut() {
            Some(v) => v,
            None => {
                self.min_max = Some((point.clone(), point.clone()));
                return;
            }
        };

        // TODO: Use cwise_min_assign and cwise_max_assign
        for i in 0..point.len() {
            min[i] = f32::min(min[i], point[i]);
            max[i] = f32::max(max[i], point[i]);
        }
    }

    pub fn build(&self) -> BoundingBox<D> {
        let (min, max) = match self.min_max.clone() {
            Some(v) => v,
            None => (VectorNew::null(), VectorNew::null()),
        };

        BoundingBox { min, max }
    }
}

/// Axis aligned bounding box.
#[derive(Debug)]
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
        let mut builder = BoundingBoxBuilder::new();

        for p in points {
            builder.update(p);
        }

        builder.build()
    }

    pub fn clip(mut self, clipbox: &BoundingBox<D>) -> Self {
        for i in 0..self.min.len() {
            self.min[i] = f32::max(self.min[i], clipbox.min[i]);
            self.max[i] = f32::min(self.max[i], clipbox.max[i]);
        }

        self
    }
}
