use math::matrix::{VectorXf, Matrix3f, Vector3f};
use math::vecxf;

/// Plane of the form 'Ax + By + C = z'
#[derive(Debug)]
pub struct Plane {
    pub a: f32,
    pub b: f32,
    pub c: f32,
}

impl Plane {

    /// Fast fitting of a plane that is horizontally flat to a set of points that is
    /// nearly flat.
    pub fn fit_near_flat(points: &[VectorXf]) -> Option<Self> {

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

        let n = points.len() as f32;

        let mat = Matrix3f::from_slice(&[
            sum_xx, sum_xy, sum_x,
            sum_xy, sum_yy, sum_y,
            sum_x,  sum_y,  n
        ]);

        let col = Vector3f::from_slice(&[
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

    pub fn compute_z(&self, x: f32, y: f32) -> f32 {
        self.a * x + self.b * y + self.c
    }

}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn already_flat_vertical() {
        let data = vec![
            vecxf!(1., 1., 0.),
            vecxf!(1., 1., 1.),
            vecxf!(1., 0., 1.),
            vecxf!(1., 0., 0.),
        ];

        assert!(Plane::fit_near_flat(&data).is_none());

    }

    #[test]
    fn already_flat() {
        let data = vec![
            vecxf!(1., 0., 1.),
            vecxf!(1., 1., 1.),
            vecxf!(0., 1., 1.),
            vecxf!(0., 0., 1.),
        ];

        let plane = Plane::fit_near_flat(&data).unwrap();

        println!("Plane: {:?}", plane);

    }

    #[test]
    fn works() {
        // https://github.com/VoronDesign/Voron-0/blob/Voron0.2r1/Drawings/Buildplate_v0.2.PDF
        let holes = vec![
            vecxf!(5.0, 115.0), // Top-left
            vecxf!(115.0, 115.0), // Top right
            vecxf!(60.0, 5.0), // Bottom
        ];

        let data = vec![
            vecxf!(5.0000, 5.0000, 0.2300),
            vecxf!(20.7125, 5.0000, 0.1925),
            vecxf!(36.4250, 5.0000, 0.1925),
            vecxf!(52.1375, 5.0000, 0.2175),
            vecxf!(67.8500, 5.0000, 0.3775),
            vecxf!(83.5625, 5.0000, 0.2750),
            vecxf!(99.2750, 5.0000, 0.2825),
            vecxf!(115.0000, 5.0000, 0.3875),
            vecxf!(115.0000, 20.7125, 0.2625),
            vecxf!(99.2875, 20.7125, 0.2300),
            vecxf!(83.5750, 20.7125, 0.1975),
            vecxf!(67.8625, 20.7125, 0.1900),
            vecxf!(52.1500, 20.7125, 0.1275),
            vecxf!(36.4375, 20.7125, 0.0850),
            vecxf!(20.7188, 20.7188, 0.1100),
            vecxf!(5.0062, 20.7188, 0.1150),
            vecxf!(5.0000, 36.4250, 0.0575),
            vecxf!(20.7062, 36.4313, 0.0400),
            vecxf!(36.4250, 36.4250, 0.0350),
            vecxf!(52.1375, 36.4250, 0.0750),
            vecxf!(67.8500, 36.4250, 0.1075),
            vecxf!(83.5687, 36.4313, 0.1500),
            vecxf!(99.2812, 36.4313, 0.1550),
            vecxf!(114.9938, 36.4313, 0.2025),
            vecxf!(115.0000, 52.1375, 0.1325),
            vecxf!(99.2937, 52.1437, 0.1050),
            vecxf!(83.5813, 52.1437, 0.0725),
            vecxf!(67.8625, 52.1375, 0.0425),
            vecxf!(52.1437, 52.1437, 0.0250),
            vecxf!(36.4313, 52.1437, -0.0175),
            vecxf!(20.7188, 52.1437, 0.0300),
            vecxf!(5.0062, 52.1438, 0.0300),
            vecxf!(5.0000, 67.8500, -0.0425),
            vecxf!(20.7062, 67.8562, -0.0500),
            vecxf!(36.4188, 67.8562, -0.0225),
            vecxf!(52.1375, 67.8625, -0.0325),
            vecxf!(67.8562, 67.8562, -0.0250),
            vecxf!(83.5687, 67.8562, 0.0375),
            vecxf!(99.2812, 67.8562, 0.0500),
            vecxf!(114.9938, 67.8562, 0.0650),
            vecxf!(115.0000, 83.5625, 0.0300),
            vecxf!(99.2937, 83.5687, -0.0025),
            vecxf!(83.5750, 83.5750, -0.0225),
            vecxf!(67.8625, 83.5750, -0.0575),
            vecxf!(52.1500, 83.5750, -0.0750),
            vecxf!(36.4313, 83.5687, -0.1025),
            vecxf!(20.7188, 83.5687, -0.0550),
            vecxf!(5.0062, 83.5687, -0.0750),
            vecxf!(5.0000, 99.2750, -0.1100),
            vecxf!(20.7125, 99.2875, -0.0875),
            vecxf!(36.4250, 99.2875, -0.1550),
            vecxf!(52.1375, 99.2875, -0.1250),
            vecxf!(67.8500, 99.2875, -0.1000),
            vecxf!(83.5625, 99.2875, -0.0475),
            vecxf!(99.2812, 99.2812, -0.0400),
            vecxf!(114.9938, 99.2812, 0.0350),
            vecxf!(115.0000, 115.0000, -0.0500),
            vecxf!(99.2875, 115.0000, -0.1375),
            vecxf!(83.5750, 115.0000, -0.1375),
            vecxf!(67.8625, 115.0000, -0.1375),
            vecxf!(52.1500, 115.0000, -0.1950),
            vecxf!(36.4375, 115.0000, -0.1625),
            vecxf!(20.7250, 115.0000, -0.2075),
            vecxf!(5.0000, 115.0000, -0.1300),
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
