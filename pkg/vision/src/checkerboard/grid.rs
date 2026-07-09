use math::matrix::{vec3d, Vector3d};

pub fn generate_checkerboard_grid_3d(
    grid_width: usize, grid_height: usize, square_size: f64
) -> Vec<Vector3d> {
    let mut points_3d = vec![];
    for i in 0..grid_height {
        for j in 0..grid_width {
            points_3d.push(vec3d(
                (j as f64) * square_size, (i as f64) * square_size, 0.0 
            ));
        }
    }

    points_3d
}