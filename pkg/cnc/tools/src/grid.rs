
pub fn generate_mesh_points(
    min_pos: (f32, f32),
    max_pos: (f32, f32),
    x_points: usize,
    y_points: usize
) -> Vec<(f32, f32)> {

    let x_unit = (max_pos.0 - min_pos.0) / ((x_points - 1) as f32);
    let y_unit = (max_pos.1 - min_pos.1) / ((y_points - 1) as f32);

    let mut out = vec![];

    for i in 0..y_points {
        let mut j_range = (0..x_points).collect::<Vec<usize>>();
        if i % 2 == 1 {
            j_range.reverse();
        }

        for j in j_range {
            out.push((
                min_pos.0 + (j as f32) * x_unit,
                min_pos.1 + (i as f32) * y_unit
            ));
        }
    }

    out
}
