use math::matrix::{VectorXd, Matrix3d, Vector3d};
use math::vecxd;

/// Plane of the form 'Ax + By + C = z'
#[derive(Debug)]
pub struct Plane {
    pub a: f64,
    pub b: f64,
    pub c: f64,
}

impl Plane {

    /// Fast fitting of a plane that is horizontally flat to a set of points that is
    /// nearly flat.
    pub fn fit_near_flat(points: &[VectorXd]) -> Option<Self> {

        let mut sum_xx = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_yy = 0.0;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_z = 0.0;
        let mut sum_xz = 0.0;
        let mut sum_yz = 0.0;

        for p in points {
            let x = p.x();
            let y = p.y();
            let z = p.z();

            sum_xx += x * x;
            sum_xy += x * y;
            sum_yy += y * y;
            sum_x  += x;
            sum_y  += y;
            sum_z  += z;
            sum_xz += x * z;
            sum_yz += y * z;
        }

        let n = points.len() as f64;

        let mat = Matrix3d::from_slice(&[
            sum_xx, sum_xy, sum_x,
            sum_xy, sum_yy, sum_y,
            sum_x,  sum_y,  n
        ]);

        let col = Vector3d::from_slice(&[
            sum_xz, sum_yz, sum_z
        ]);

        // TODO: Wrap this calculation into the inverse operation.
        if mat.determinant().abs() < 0.0001 {
            return None;
        }

        let params = mat.inverse() * col;

        Some(Self {
            a: params[0],
            b: params[1],
            c: params[2]
        })
    }

    pub fn compute_z(&self, x: f64, y: f64) -> f64 {
        self.a * x + self.b * y + self.c
    }

}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn already_flat_vertical() {
        let data = vec![
            vecxd!(1., 1., 0.),
            vecxd!(1., 1., 1.),
            vecxd!(1., 0., 1.),
            vecxd!(1., 0., 0.),
        ];

        assert!(Plane::fit_near_flat(&data).is_none());

    }

    #[test]
    fn already_flat() {
        let data = vec![
            vecxd!(1., 0., 1.),
            vecxd!(1., 1., 1.),
            vecxd!(0., 1., 1.),
            vecxd!(0., 0., 1.),
        ];

        let plane = Plane::fit_near_flat(&data).unwrap();

        println!("Plane: {:?}", plane);

    }

    #[test]
    fn works() {
        // https://github.com/VoronDesign/Voron-0/blob/Voron0.2r1/Drawings/Buildplate_v0.2.PDF
        let holes = vec![
            vecxd!(5.0, 115.0), // Top-left
            vecxd!(115.0, 115.0), // Top right
            vecxd!(60.0, 5.0), // Bottom
        ];

        let data = vec![
            vecxd!(5.0000, 5.0000, 0.2300),
            vecxd!(20.7125, 5.0000, 0.1925),
            vecxd!(36.4250, 5.0000, 0.1925),
            vecxd!(52.1375, 5.0000, 0.2175),
            vecxd!(67.8500, 5.0000, 0.3775),
            vecxd!(83.5625, 5.0000, 0.2750),
            vecxd!(99.2750, 5.0000, 0.2825),
            vecxd!(115.0000, 5.0000, 0.3875),
            vecxd!(115.0000, 20.7125, 0.2625),
            vecxd!(99.2875, 20.7125, 0.2300),
            vecxd!(83.5750, 20.7125, 0.1975),
            vecxd!(67.8625, 20.7125, 0.1900),
            vecxd!(52.1500, 20.7125, 0.1275),
            vecxd!(36.4375, 20.7125, 0.0850),
            vecxd!(20.7188, 20.7188, 0.1100),
            vecxd!(5.0062, 20.7188, 0.1150),
            vecxd!(5.0000, 36.4250, 0.0575),
            vecxd!(20.7062, 36.4313, 0.0400),
            vecxd!(36.4250, 36.4250, 0.0350),
            vecxd!(52.1375, 36.4250, 0.0750),
            vecxd!(67.8500, 36.4250, 0.1075),
            vecxd!(83.5687, 36.4313, 0.1500),
            vecxd!(99.2812, 36.4313, 0.1550),
            vecxd!(114.9938, 36.4313, 0.2025),
            vecxd!(115.0000, 52.1375, 0.1325),
            vecxd!(99.2937, 52.1437, 0.1050),
            vecxd!(83.5813, 52.1437, 0.0725),
            vecxd!(67.8625, 52.1375, 0.0425),
            vecxd!(52.1437, 52.1437, 0.0250),
            vecxd!(36.4313, 52.1437, -0.0175),
            vecxd!(20.7188, 52.1437, 0.0300),
            vecxd!(5.0062, 52.1438, 0.0300),
            vecxd!(5.0000, 67.8500, -0.0425),
            vecxd!(20.7062, 67.8562, -0.0500),
            vecxd!(36.4188, 67.8562, -0.0225),
            vecxd!(52.1375, 67.8625, -0.0325),
            vecxd!(67.8562, 67.8562, -0.0250),
            vecxd!(83.5687, 67.8562, 0.0375),
            vecxd!(99.2812, 67.8562, 0.0500),
            vecxd!(114.9938, 67.8562, 0.0650),
            vecxd!(115.0000, 83.5625, 0.0300),
            vecxd!(99.2937, 83.5687, -0.0025),
            vecxd!(83.5750, 83.5750, -0.0225),
            vecxd!(67.8625, 83.5750, -0.0575),
            vecxd!(52.1500, 83.5750, -0.0750),
            vecxd!(36.4313, 83.5687, -0.1025),
            vecxd!(20.7188, 83.5687, -0.0550),
            vecxd!(5.0062, 83.5687, -0.0750),
            vecxd!(5.0000, 99.2750, -0.1100),
            vecxd!(20.7125, 99.2875, -0.0875),
            vecxd!(36.4250, 99.2875, -0.1550),
            vecxd!(52.1375, 99.2875, -0.1250),
            vecxd!(67.8500, 99.2875, -0.1000),
            vecxd!(83.5625, 99.2875, -0.0475),
            vecxd!(99.2812, 99.2812, -0.0400),
            vecxd!(114.9938, 99.2812, 0.0350),
            vecxd!(115.0000, 115.0000, -0.0500),
            vecxd!(99.2875, 115.0000, -0.1375),
            vecxd!(83.5750, 115.0000, -0.1375),
            vecxd!(67.8625, 115.0000, -0.1375),
            vecxd!(52.1500, 115.0000, -0.1950),
            vecxd!(36.4375, 115.0000, -0.1625),
            vecxd!(20.7250, 115.0000, -0.2075),
            vecxd!(5.0000, 115.0000, -0.1300),
        ];

        let plane = Plane::fit_near_flat(&data).unwrap();

        println!("Plane: {:?}", plane);



        for pt in &data {
            let z = plane.compute_z(pt.x(), pt.y());
            let delta = z - pt.z();
            println!("{}", delta);
        }

        /*
        +Z means the point is too low (must raise)
        -Z means the point is too high (must lower)
        */
        for pt in &holes {
            let z = plane.compute_z(pt.x(), pt.y());
            println!("Hole Z: {}", z);
        }

    }


}
