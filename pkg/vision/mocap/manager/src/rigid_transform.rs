use math::matrix::{Matrix3d, Vector3d, MatrixXd, Dynamic};
use math::matrix::svd::*;


/// Computes a rotation/translation transformation such that
/// 'output_point = R*input_point + translation'. 
///
/// See https://igl.ethz.ch/projects/ARAP/svd_rot.pdf
///
/// TODO: Have some fast method for 3 points.
pub fn find_rigid_transform(
    input_points: &[Vector3d],
    output_points: &[Vector3d],
    weights: &[f64]
) -> (Matrix3d, Vector3d) {
    let input_center = centroid(input_points, weights);
    let output_center = centroid(output_points, weights);

    let n = input_points.len();
    let mut input_mat = MatrixXd::zero_with_shape(n, 3);
    let mut output_mat = MatrixXd::zero_with_shape(n, 3);

    for i in 0..n {
        let w = if i < weights.len() { weights[i] } else { 1.0 };
        input_mat.row_mut(i).copy_from_slice(&((&input_points[i] - &input_center) * w).as_ref());
        output_mat.row_mut(i).copy_from_slice(&((&output_points[i] - &output_center) * w).as_ref());
    }

    let covar = input_mat.as_transpose() * output_mat;

    // In Eigen: JacobiSVD<MatrixXd> svd(H, ComputeThinU | ComputeThinV);
    let svd = SVD::eigen_svd(&covar);

    let m_new = &svd.u * &svd.s * svd.v.transpose();

    let mut r = &svd.v * svd.u.as_transpose();

    let r_det = r.determinant();
    if r_det < 0.0 {
        let mut x = Matrix3d::identity();
        x[(2, 2)] = r_det;
        r = &svd.v * x * svd.u.as_transpose();
    }

    let mut r3x3 = Matrix3d::zero();
    r3x3.copy_from_slice(r.as_ref());

    let t = output_center + (&r3x3 * input_center * -1.0);

    let mut error = 0.0;
    for i in 0..input_points.len() {
        let out1 = &r3x3 * &input_points[i] + &t;
        error += (out1 - &output_points[i]).norm_squared();
    }
    error /= input_points.len() as f64;

    (r3x3, t)
}


fn centroid(points: &[Vector3d], weights: &[f64]) -> Vector3d {
    let mut sum = Vector3d::zero();
    let mut weight_sum = 0.0;
    for (i, pt) in points.iter().enumerate() {
        let w = if i < weights.len() { weights[i] } else { 1.0 };

        sum += pt.clone() * w;
        weight_sum += w;
    }

    sum /= weight_sum;
    sum
}


#[cfg(test)]
mod tests {

    use super::*;

    use math::matrix::vec3d;

    #[test]
    fn works() {
        // This is a test case where r_det is < 0 so needs to be flipped.

        let input_points = vec![
            vec3d(0.1562589618242893, 0.05000366187520844, -0.000011452674713046108), vec3d(0.031222561370267413, -0.15001454644049908, 0.000003531063835088848), vec3d(-0.21874464991354822, 0.05000977880022214, -0.000004037655559359085), vec3d(0.031263126718991466, 0.0500011057650685, 0.000011959266437316347)
        ];

        let output_points = vec![
            vec3d(0.12494343307093686, 0.002370286525580113, 0.003021383519939095), vec3d(0.003772910448230819, -0.19995211985693978, 0.0030230234512506893), vec3d(-0.24994749436491168, -0.004728401913479398, 0.0030015366018398618), vec3d(-0.0000014224427236299227, -0.00002994383347936077, 0.0029897244639278666)
        ];

        let (r, t) = find_rigid_transform(&input_points, &output_points, &[]);

        // println!("")


    }


}
