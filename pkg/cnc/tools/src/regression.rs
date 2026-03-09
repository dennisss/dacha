use std::f64::consts::PI;

use math::matrix::MatrixXd;

pub fn linear_regression(data: &[(f64, f64)]) -> (f64, f64) {
    let mut a = MatrixXd::zero_with_shape(data.len(), 2);
    let mut b = MatrixXd::zero_with_shape(data.len(), 1);
    for i in 0..data.len() {
        a[(i, 0)] = data[i].0;
        a[(i, 1)] = 1.0;
        b[i] = data[i].1
    }

    let x = pinv(&a) * b;


    (x[0], x[1])
}



/// see the "Sine-cosine form" of https://en.wikipedia.org/wiki/Fourier_series
#[derive(Clone)]
pub struct FourierRegression {
    weights: MatrixXd,
}

impl FourierRegression {

    pub fn clear_dc_offset(&mut self) {
        self.weights[0] = 0.0;
    }

    /// Note that the angles (first data point) should range from 0 to 1
    pub fn create(data: &[(f64, f64)], num_harmonics: usize) -> Self {

        let mut x = MatrixXd::zero_with_shape(data.len(), 1 + 2 * num_harmonics);
        let mut b = MatrixXd::zero_with_shape(data.len(), 1);

        for i in 0..data.len() {
            x[(i, 0)] = 1.0; // dc offset

            for j in 0..num_harmonics {
                let angle = 2.0 * PI * ((j + 1) as f64) * data[i].0;
                
                x[(i, 1 + 2*j)] = angle.cos();
                x[(i, 1 + 2*j + 1)] = angle.sin();
            }

            b[i] = data[i].1;
        }

        let inv = pinv(&x);
        let out = inv * &b;

        Self {
            weights: out
        }
    }

    pub fn compute(&self, angle: f64) -> f64 {
        let mut out = self.weights[0];

        for i in 0..((self.weights.len() - 1) / 2) {
            let a = 2.0 * PI * ((i + 1) as f64) * angle;
                
            out += self.weights[1 + 2*i] * a.cos();
            out += self.weights[1 + 2*i + 1] * a.sin();
        }

        out
    }

}


pub fn pinv(x: &MatrixXd) -> MatrixXd {
    (x.transpose() * x).inverse() * x.transpose()
    // x.transpose() * (x * x.transpose()).inverse()
}
