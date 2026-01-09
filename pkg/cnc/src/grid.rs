use alloc::vec::Vec;

use common::errors::*;
use cnc_motion_proto::cnc::{GridProto, GridValuesProto};


#[derive(Clone)]
pub struct Grid {
    proto: GridProto
}

impl Grid {
    pub fn create(
        min_pos: (f32, f32),
        max_pos: (f32, f32),
        x_count: usize,
        y_count: usize    
    ) -> Self {
        let x_interval = (max_pos.0 - min_pos.0) / ((x_count - 1) as f32);
        let y_interval = (max_pos.1 - min_pos.1) / ((y_count - 1) as f32);

        let mut proto = GridProto::default();
        proto.set_base_point_x(min_pos.0);
        proto.set_base_point_y(min_pos.1);
        proto.set_x_interval(x_interval);
        proto.set_y_interval(y_interval);
        proto.set_x_count(x_count as u32);
        proto.set_y_count(y_count as u32);

        Self {
            proto
        }
    }

    pub fn x_interval(&self) -> f32 {
        self.proto.x_interval()
    }

    pub fn y_interval(&self) -> f32 {
        self.proto.y_interval()
    }

    pub fn to_proto(&self) -> GridProto {
        self.proto.clone()
    }

    // TODO: Need some validations.
    pub fn from_proto(proto: &GridProto) -> Self {
        Self { proto: proto.clone() }
    }

    pub fn scan_order(&self) -> Vec<(f32, f32)> {
        let mut out = vec![];

        for (i, j) in self.scan_order_indexes() {
            out.push((
                self.proto.base_point_x() + (j as f32) * self.proto.x_interval(),
                self.proto.base_point_y() + (i as f32) * self.proto.y_interval()
            ));
        }

        out
    }

    fn scan_order_indexes(&self) -> Vec<(usize, usize)> {
        let mut out = vec![];

        for i in 0..(self.proto.y_count() as usize) {
            let mut j_range = (0..(self.proto.x_count() as usize)).collect::<Vec<usize>>();
            if i % 2 == 1 {
                j_range.reverse();
            }

            for j in j_range {
                out.push((i, j));
            }
        }

        out
    }

    fn position(&self, i: usize, j: usize) -> (f32, f32) {
        (
            self.proto.base_point_x() + (j as f32) * self.proto.x_interval(),
            self.proto.base_point_y() + (i as f32) * self.proto.y_interval()
        )
    }
}

pub struct GridValues {
    grid: Grid,

    // TODO: Just make this a MatrixXf
    values: Vec<Vec<f32>>,
}

impl GridValues {

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    pub fn iter(&self) -> Vec<(f32, f32, f32)> {
        let mut out = vec![];
        
        for i in 0..self.values.len() {
            for j in 0..self.values[i].len() {
                let (x, y) = self.grid.position(i, j);
                let z = self.values[i][j];

                out.push((x,y,z));
            }
        }

        out
    } 

    pub fn to_proto(&self) -> GridValuesProto {
        let mut out = GridValuesProto::default();
        out.set_grid(self.grid.to_proto());
        for row in &self.values {
            out.values_mut().extend_from_slice(&row[..]);
        }
        out
    }

    // TODO: Need some validations on the size of values.
    pub fn from_proto(proto: &GridValuesProto) -> Self {
        let mut values = vec![];
        for row in proto.values().chunks(proto.grid().x_count() as usize) {
            values.push(row.to_vec());
        }
        
        Self {
            grid: Grid::from_proto(proto.grid()),
            values
        }
    }

    pub fn from_scan_values(grid: Grid, raw_values: &[f32]) -> Result<Self> {
        let mut values = vec![];
        for _ in 0..grid.proto.y_count() {
            let mut row = vec![];
            row.resize(grid.proto.x_count() as usize, 0.0);
            values.push(row);
        }

        let indexes = grid.scan_order_indexes();
        if indexes.len() != raw_values.len() {
            return Err(err_msg("Wrong number of values"));
        }

        for ((i,j), value) in indexes.into_iter().zip(raw_values.iter().cloned()) {
            values[i][j] = value;
        }

        Ok(Self {
            grid,
            values
        })
    }


    /// Given an (x,y) position, interpolates a value at that position in the grid
    /// using bilinear interpolation of nearby values.
    ///
    /// NOTE: (x,y) points outside of the grid will linearly interpolate the nearest
    /// 1 or 2 points.
    pub fn interpolate_value(&self, mut x: f32, mut y: f32) -> f32 {
        x -= self.grid.proto.base_point_x();
        y -= self.grid.proto.base_point_y();

        let grid_cols = self.grid.proto.x_count() as usize;
        let grid_rows = self.grid.proto.y_count() as usize;
        let grid_width = self.grid.proto.x_interval() * ((grid_cols - 1) as f32);
        let grid_height = self.grid.proto.y_interval() * ((grid_rows - 1) as f32);

        let x_coord = x / self.grid.proto.x_interval();
        let y_coord = y / self.grid.proto.y_interval();

        let x0 = x_coord.floor().max(0.0).min((grid_cols - 1) as f32);
        let x1 = x_coord.ceil().max(0.0).min((grid_cols - 1) as f32);
        let x0_alpha = 1.0 - (x_coord - x0);

        let y0 = y_coord.floor().max(0.0).min((grid_rows - 1) as f32);
        let y1 = y_coord.ceil().max(0.0).min((grid_rows - 1) as f32);
        let y0_alpha = 1.0 - (y_coord - y0);
        
        let a = interp(
            self.values[y0 as usize][x0 as usize],
            self.values[y0 as usize][x1 as usize],
            x0_alpha
        );
        let b = interp(
            self.values[y1 as usize][x0 as usize],
            self.values[y1 as usize][x1 as usize],
            x0_alpha
        );

        interp(
            a, b, y0_alpha
        )
    }
}

pub fn interp(a: f32, b: f32, a_alpha: f32) -> f32 {
    a * a_alpha + b * (1.0 - a_alpha)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_scan_5x5() {

        let grid = Grid::create((1., 1.), (5., 5.), 5, 5);

        assert_eq!(grid.scan_order(), vec![
            (1.0, 1.0), (2.0, 1.0), (3.0, 1.0), (4.0, 1.0), (5.0, 1.0),
            (5.0, 2.0), (4.0, 2.0), (3.0, 2.0), (2.0, 2.0), (1.0, 2.0),
            (1.0, 3.0), (2.0, 3.0), (3.0, 3.0), (4.0, 3.0), (5.0, 3.0),
            (5.0, 4.0), (4.0, 4.0), (3.0, 4.0), (2.0, 4.0), (1.0, 4.0),
            (1.0, 5.0), (2.0, 5.0), (3.0, 5.0), (4.0, 5.0), (5.0, 5.0)
        ]);
    }

    #[test]
    fn grid_interp() {
        let grid = Grid::create((1., 1.), (3., 3.), 3, 3);
        assert_eq!(grid.scan_order(), vec![
            (1.0, 1.0), (2.0, 1.0), (3.0, 1.0),
            (3.0, 2.0), (2.0, 2.0), (1.0, 2.0),
            (1.0, 3.0), (2.0, 3.0), (3.0, 3.0)
        ]);

        // No interp
        {
            let grid_values = GridValues::from_scan_values(grid.clone(), &[
                10.0, 10.0, 10.0,
                10.0, 10.0, 10.0,
                10.0, 10.0, 10.0,
            ]).unwrap();

            assert_eq!(grid_values.interpolate_value(0.0, 0.0), 10.0);
            assert_eq!(grid_values.interpolate_value(1.0, 0.0), 10.0);
            assert_eq!(grid_values.interpolate_value(1.5, 0.0), 10.0);
            assert_eq!(grid_values.interpolate_value(1.5, 1.5), 10.0);
            assert_eq!(grid_values.interpolate_value(1.5, 1.5), 10.0);
        }

        // Just Y interp
        {
            let grid_values = GridValues::from_scan_values(grid.clone(), &[
                10.0, 10.0, 10.0,
                20.0, 20.0, 20.0,
                20.0, 20.0, 20.0,
            ]).unwrap();

            assert_eq!(grid_values.interpolate_value(0.0, 0.0), 10.0);
            assert_eq!(grid_values.interpolate_value(0.0, 3.0), 20.0);
            assert_eq!(grid_values.interpolate_value(0.0, 1.5), 15.0);
            assert_eq!(grid_values.interpolate_value(1.5, 1.5), 15.0);
            assert_eq!(grid_values.interpolate_value(2.5, 1.5), 15.0);
            assert_eq!(grid_values.interpolate_value(2.5, 2.5), 20.0);

            assert_eq!(grid_values.interpolate_value(1.5, 1.25), 12.5);
            assert_eq!(grid_values.interpolate_value(1.5, 1.75), 17.5);
        }

        // Just X interp
        {
            let grid_values = GridValues::from_scan_values(grid.clone(), &[
                10.0, 20.0, 30.0,
                30.0, 20.0, 10.0,
                10.0, 20.0, 30.0,
            ]).unwrap();

            assert_eq!(grid_values.interpolate_value(0.0, 0.0), 10.0);
            assert_eq!(grid_values.interpolate_value(3.0, 0.0), 30.0);
            assert_eq!(grid_values.interpolate_value(4.0, 0.0), 30.0);
            assert_eq!(grid_values.interpolate_value(1.5, 0.0), 15.0);
            assert_eq!(grid_values.interpolate_value(1.5, 1.5), 15.0);
            assert_eq!(grid_values.interpolate_value(1.5, 2.5), 15.0);
            assert_eq!(grid_values.interpolate_value(2.5, 0.0), 25.0);
            assert_eq!(grid_values.interpolate_value(2.5, 1.5), 25.0);
            assert_eq!(grid_values.interpolate_value(2.5, 2.5), 25.0);

            assert_eq!(grid_values.interpolate_value(1.25, 1.5), 12.5);
            assert_eq!(grid_values.interpolate_value(1.75, 1.5), 17.5);

        }

        // TODO: Test full X-Y interpolation
    }

}
