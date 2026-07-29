// TODO: Verify nothing in here can overflow in u64 or in f64 int limits..
// We may need to limit max blob size before calculating stats and/or
// compute sums using relative coordinates. 

#[derive(Debug, Clone)]
pub struct ComponentData {
    pub min_x: u16,
    pub max_x: u16,
    pub min_y: u16,
    pub max_y: u16,

    /// Total number of pixels in this component.
    pub area: u32,

    /// Sum of all pixel intensities in the component.
    pub mass: u64,

    /// Sum of x coordinates weighted by pixel intensities.
    pub moment_x: u64,

    /// Sum of y coordinates weighted by pixel intensities.
    pub moment_y: u64,

    /// Sum of x*x coordinates weighted by pixel intensities.
    pub moment_xx: u64,

    /// Sum of x*y coordinates weighted by pixel intensities.
    pub moment_xy: u64,

    /// Sum of y*y coordinates weighted by pixel intensities.
    pub moment_yy: u64,
}

impl ComponentData {
    pub fn empty() -> Self {
        Self {
            min_x: std::u16::MAX,
            min_y: std::u16::MAX,
            max_x: 0,
            max_y: 0,
            area: 0,
            mass: 0,
            moment_x: 0,
            moment_y: 0,
            moment_xx: 0,
            moment_xy: 0,
            moment_yy: 0,
        }
    }

    pub fn add_pixel(&mut self, x: usize, y: usize, intensity: u64) {
        self.min_x = self.min_x.min(x as u16);
        self.max_x = self.max_x.max(x as u16);
        self.min_y = self.min_y.min(y as u16);
        self.max_y = self.max_y.max(y as u16);

        let x = x as u64;
        let y = y as u64;

        self.area += 1;
        self.mass += intensity;

        let xi = x * intensity;
        self.moment_x += xi;
        self.moment_xx += x * xi;

        let yi = y * intensity;
        self.moment_y += yi;
        self.moment_yy += y * yi;
        self.moment_xy += x * yi;
    }

    pub fn start_pixel_row(&mut self, x: usize, y: usize) {
        self.area = 1; // Just so that other logic believes this isn't an empty data structure.
        self.min_x = x as u16;
        self.max_x = x as u16;
        self.min_y = y as u16;
        self.max_y = y as u16;
    }

    pub fn add_row_pixel(&mut self, x: usize, y: usize, intensity: u64) {
        self.max_x = x as u16;
        self.mass += intensity;
        
        let x = x as u64;
        let xi = x * intensity;
        self.moment_x += xi;
        self.moment_xx += x * xi;
    }

    pub fn finish_pixel_row(&mut self) {
        self.area = (self.max_x - self.min_x) as u32;

        let y = self.min_y as u64;
        self.moment_y = self.mass * y;
        self.moment_yy = self.moment_y * y;
        self.moment_xy = self.moment_x * y;
    }

    pub fn add(&mut self, other: &Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.max_x = self.max_x.max(other.max_x);
        self.min_y = self.min_y.min(other.min_y);
        self.max_y = self.max_y.max(other.max_y);

        self.area += other.area;
        self.mass += other.mass;
        self.moment_x += other.moment_x;
        self.moment_y += other.moment_y;
        self.moment_xx += other.moment_xx;
        self.moment_yy += other.moment_yy;
        self.moment_xy += other.moment_xy;
    }

    pub fn calculate_stats(&self) -> ComponentStats {
        let mass = self.mass as f64;
        let moment_x = self.moment_x as f64;
        let moment_y = self.moment_y as f64;
        let moment_xx = self.moment_xx as f64;
        let moment_xy = self.moment_xy as f64;
        let moment_yy = self.moment_yy as f64;

        let mass_inv = 1.0 / mass;

        let mean_x = moment_x * mass_inv;
        let mean_y = moment_y * mass_inv;

        let variance_x = (moment_xx - mean_x * moment_x) * mass_inv;
        let variance_y = (moment_yy - mean_y * moment_y) * mass_inv;
        let covariance = (moment_xy - mean_x * moment_y) * mass_inv;

        /*
        The code here is an optimized version of finding the eigenvalues and the
        angle between the eigenvectors of this matrix:
            let covar_mat = Matrix2d::from_slice(&[
                variance_x, covariance,
                covariance, variance_y
            ]);
        */

        let eigen_mean = (variance_x + variance_y) / 2.0;
        let eigen_diff = (variance_x - variance_y) / 2.0;

        let r = (eigen_diff * eigen_diff + covariance * covariance).sqrt();

        let lambda_a = eigen_mean + r;
        let lambda_b = eigen_mean - r;

        let angle = covariance.atan2(eigen_diff) / 2.0;

        ComponentStats {
            centroid_x: (mean_x + 0.5) as f32,
            centroid_y: (mean_y + 0.5) as f32,
            radius_a: (2.0 * lambda_a.sqrt()) as f32,
            radius_b: (2.0 * lambda_b.sqrt()) as f32,
            angle: angle as f32
        }

    }
}


#[derive(Clone, Debug)]
pub struct ComponentStats {
    pub centroid_x: f32,
    pub centroid_y: f32,
    pub radius_a: f32,
    pub radius_b: f32,
    pub angle: f32, 
}
