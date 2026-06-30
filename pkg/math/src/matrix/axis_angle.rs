use crate::matrix::{Vector3d, Matrix3d};

/// Converts a 3x3 rotation matrix to a 3x1 vector
///
/// See
/// https://en.wikipedia.org/wiki/Axis%E2%80%93angle_representation#Exponential_map_from_%F0%9D%94%B0%F0%9D%94%AC(3)_to_SO(3) 
/// "Log map from SO(3) to so(3)"
pub fn to_axis_angle(mat: &Matrix3d) -> Vector3d {
    let trace = mat[(0, 0)] + mat[(1, 1)] + mat[(2, 2)];
    let cos_theta = ((trace - 1.0) / 2.0).clamp(-1.0, 1.0);
    let angle = cos_theta.acos();

    if angle.abs() < 0.00001 {
        return Vector3d::from_slice(&[0.0, 0.0, 0.0]);
    }

    // trace = -1 (rotation is 180 degrees)
    if (trace + 1.0).abs() < 0.00001 {
        let x;
        let y;
        let z;

        // Find the largest diagonal element to avoid division by zero and precision loss
        if mat[(0, 0)] > mat[(1, 1)] && mat[(0, 0)] > mat[(2, 2)] {
            // mat[(0, 0)] is the largest
            x = ((mat[(0, 0)] + 1.0) / 2.0).max(0.0001).sqrt();
            y = (mat[(0, 1)] + mat[(1, 0)]) / (4.0 * x);
            z = (mat[(0, 2)] + mat[(2, 0)]) / (4.0 * x);
        } else if mat[(1, 1)] > mat[(2, 2)] {
            // mat[(1, 1)] is the largest
            y = ((mat[(1, 1)] + 1.0) / 2.0).max(0.0001).sqrt();
            x = (mat[(0, 1)] + mat[(1, 0)]) / (4.0 * y);
            z = (mat[(1, 2)] + mat[(2, 1)]) / (4.0 * y);
        } else {
            // mat[(2, 2)] is the largest
            z = ((mat[(2, 2)] + 1.0) / 2.0).max(0.0001).sqrt();
            x = (mat[(0, 2)] + mat[(2, 0)]) / (4.0 * z);
            y = (mat[(1, 2)] + mat[(2, 1)]) / (4.0 * z);
        }

        let mut axis = Vector3d::from_slice(&[x, y, z]).normalized();

        // Recover the correct sign of the axis using the anti-symmetric part
        let anti_sym = Vector3d::from_slice(&[
            mat[(2, 1)] - mat[(1, 2)],
            mat[(0, 2)] - mat[(2, 0)],
            mat[(1, 0)] - mat[(0, 1)],
        ]);

        // If the dot product is negative, our .sqrt() choice flipped the true axis
        if axis.dot(&anti_sym) < 0.0 {
            axis = axis * -1.0;
        }

        // Return the exact angle, not a hardcoded PI
        return axis * angle;
    }

    let axis = Vector3d::from_slice(&[
        mat[(2, 1)] - mat[(1, 2)],
        mat[(0, 2)] - mat[(2, 0)],
        mat[(1, 0)] - mat[(0, 1)],
    ]).normalized();

    axis * angle
}


/// See https://en.wikipedia.org/wiki/Rodrigues%27_rotation_formula
pub fn from_axis_angle(axis_angle: &Vector3d) -> Matrix3d {
    let angle = axis_angle.norm();
    let axis = axis_angle.clone().normalized();

    let k = Matrix3d::from_slice(&[
        0., -axis.z(), axis.y(),
        axis.z(), 0., -axis.x(),
        -axis.y(), axis.x(), 0.
    ]);

    Matrix3d::identity() +
    k.clone() * angle.sin() +
    (&k * &k) * (1.0 - angle.cos())
}


/// Rotates a point by a rotation represented in axis-angle form.
///
/// See https://en.wikipedia.org/wiki/Rodrigues%27_rotation_formula
pub fn rotate_by_axis_angle(point: &Vector3d, axis_angle: &Vector3d) -> Vector3d {
    let angle = axis_angle.norm();
    if angle.abs() <= 0.00001 {
        return point.clone();
    }

    let axis = axis_angle.clone().normalized();

    (point.clone() * angle.cos()) +
    (axis.cross(point) * angle.sin()) +
    (axis.clone() * axis.dot(point) * (1.0 - angle.cos()))
}

/// Like rotate_by_axis_angle except is an approximation for small non-zero angles.
///
/// This function is much more stable when calculating gradients w.r.t the axis_angle input.
pub fn rotate_by_axis_angle_near_zero(point: &Vector3d, axis_angle: &Vector3d) -> Vector3d {
    point + axis_angle.cross(point)
}
