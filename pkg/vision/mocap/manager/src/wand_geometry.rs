//! Diagnostics for checking how well a physical calibration wand matches its
//! configured geometry.

use common::errors::*;
use math::matrix::{Vector2d, Vector3d, Matrix3d, Matrix4d};
use mocap_proto::mocap::*;
use math_proto_util::VectorProtoExt;
use vision::{BundleAdjustmentSolution, ObjectPointViewResidual};

use crate::wand::NUM_WAND_MARKERS;

pub const WAND_MARKER_NAMES: [&'static str; NUM_WAND_MARKERS] = ["left", "middle", "right", "bottom"];

/// Rigid transform from the configured wand frame to a canonical wand frame
/// where the middle marker is at the origin, the left marker is on the -X
/// axis and the bottom marker is in the XY plane with negative Y.
///
/// For the nominal T shape, this is the identity.
///
/// Used to fix the gauge freedom when optimizing the wand marker positions.
#[derive(Clone, Debug)]
pub struct WandGauge {
    /// Rows are the canonical axes expressed in the configured frame.
    rotation: Matrix3d,

    /// Position of the middle marker in the configured frame.
    origin: Vector3d,
}

impl WandGauge {
    /// 'points' are in the order of wand_reference_points().
    pub fn from_points(points: &[Vector3d]) -> Result<Self> {
        if points.len() != NUM_WAND_MARKERS {
            return Err(err_msg("Wrong number of wand points"));
        }

        let origin = points[1].clone();

        let x_dir = &points[1] - &points[0];
        if x_dir.norm() < 1e-6 {
            return Err(err_msg("Left and middle wand markers are at the same position"));
        }
        let x_axis = x_dir.normalized();

        let bottom = &points[3] - &points[1];
        let bottom_perp = bottom.clone() - (x_axis.clone() * bottom.dot(&x_axis));
        if bottom_perp.norm() < 1e-6 {
            return Err(err_msg("Bottom wand marker is collinear with the left/middle markers"));
        }
        let y_axis = (bottom_perp * -1.0).normalized();

        let z_axis = x_axis.cross(&y_axis).normalized();

        let mut rotation = Matrix3d::zero();
        for j in 0..3 {
            rotation[(0, j)] = x_axis[j];
            rotation[(1, j)] = y_axis[j];
            rotation[(2, j)] = z_axis[j];
        }

        Ok(Self { rotation, origin })
    }

    pub fn to_canonical(&self, point: &Vector3d) -> Vector3d {
        &self.rotation * (point - &self.origin)
    }

    pub fn from_canonical(&self, point: &Vector3d) -> Vector3d {
        (self.rotation.transpose() * point) + &self.origin
    }

    /// Converts a direction (not a position) from the canonical frame.
    pub fn direction_from_canonical(&self, dir: &Vector3d) -> Vector3d {
        self.rotation.transpose() * dir
    }

    /// Matrix which maps canonical coordinates to configured coordinates.
    pub fn from_canonical_mat4x4(&self) -> Matrix4d {
        let mut out = Matrix4d::zero();
        out.block_mut(0, 0).copy_from(&self.rotation.transpose());
        out.block_mut(0, 3).copy_from(&self.origin);
        out[(3, 3)] = 1.0;
        out
    }
}

/// Summary of the reprojection residuals of all observations of one wand marker.
#[derive(Clone, Debug)]
pub struct WandMarkerResidualStats {
    pub marker_index: usize,
    pub num_views: usize,

    /// RMS reprojection error in pixels.
    pub rms_error: f64,

    /// Mean of (observed - projected) in pixels.
    pub mean_residual: Vector2d,

    /// Linearized least squares estimate of the displacement of this marker (in
    /// meters, in the object frame used by the bundle adjustment) which would
    /// best explain its residuals if everything else stayed fixed.
    pub offset_estimate: Option<Vector3d>,
}

impl WandMarkerResidualStats {
    pub fn to_proto(&self) -> WandMarkerResidual {
        let mut proto = WandMarkerResidual::default();
        proto.set_marker_index(self.marker_index as u32);
        proto.set_num_views(self.num_views as u32);
        proto.set_rms_error(self.rms_error);
        proto.set_mean_residual(self.mean_residual.to_proto());
        if let Some(v) = &self.offset_estimate {
            proto.set_offset_estimate(v.to_proto());
        }
        proto
    }
}

/// Groups the final reprojection residuals by wand marker.
///
/// 'view_markers[i]' is the marker index of the i'th point view added to the
/// bundle adjustment.
pub fn compute_marker_residual_stats(
    solution: &BundleAdjustmentSolution,
    view_markers: &[usize],
) -> Vec<WandMarkerResidualStats> {
    let residuals = solution.object_point_view_residuals();
    assert_eq!(residuals.len(), view_markers.len());

    let mut out = vec![];

    for marker_index in 0..NUM_WAND_MARKERS {
        let mut num_views = 0;
        let mut sum_squared = 0.0;
        let mut sum = Vector2d::zero();

        // Normal equations for the offset estimate.
        let mut jtj = Matrix3d::zero();
        let mut jtr = Vector3d::zero();

        for (r, m) in residuals.iter().zip(view_markers.iter()) {
            if *m != marker_index {
                continue;
            }

            num_views += 1;
            sum_squared += r.residual.norm_squared();
            sum += &r.residual;

            let cols = point_jacobian(solution, r);
            for i in 0..3 {
                for j in 0..3 {
                    jtj[(i, j)] += cols[i].dot(&cols[j]);
                }
                jtr[i] += cols[i].dot(&r.residual);
            }
        }

        if num_views == 0 {
            continue;
        }

        let offset_estimate = jtj.inverse().map(|inv| inv * jtr);

        out.push(WandMarkerResidualStats {
            marker_index,
            num_views,
            rms_error: (sum_squared / (num_views as f64)).sqrt(),
            mean_residual: sum / (num_views as f64),
            offset_estimate,
        });
    }

    out
}

/// Numerically computes the derivative of the projected point with respect to
/// each coordinate of the object local point.
fn point_jacobian(solution: &BundleAdjustmentSolution, r: &ObjectPointViewResidual) -> [Vector2d; 3] {
    const STEP: f64 = 1e-6; // 1 micron

    let mut cols = [Vector2d::zero(), Vector2d::zero(), Vector2d::zero()];
    for k in 0..3 {
        let mut p1 = r.point_3d.clone();
        p1[k] += STEP;
        let mut p2 = r.point_3d.clone();
        p2[k] -= STEP;

        let a = solution.project_object_point(r.camera_idx, r.object_idx, &p1);
        let b = solution.project_object_point(r.camera_idx, r.object_idx, &p2);
        cols[k] = (a - b) / (2.0 * STEP);
    }

    cols
}

fn fmt_mm(v: &Vector3d) -> String {
    format!("({:+8.3}, {:+8.3}, {:+8.3})", v.x() * 1000.0, v.y() * 1000.0, v.z() * 1000.0)
}

pub fn print_marker_residual_stats(title: &str, stats: &[WandMarkerResidualStats]) {
    println!("{}", title);
    for s in stats {
        let offset = match &s.offset_estimate {
            Some(v) => fmt_mm(v),
            None => "n/a".to_string(),
        };

        println!(
            "  marker {} ({:>6}): views {:6}  rms {:.3} px  mean ({:+.3}, {:+.3}) px  offset estimate {} mm",
            s.marker_index, WAND_MARKER_NAMES[s.marker_index], s.num_views, s.rms_error,
            s.mean_residual.x(), s.mean_residual.y(), offset
        );
    }
    println!(
        "  (A marker with a clearly larger rms or offset than the others is likely misplaced. \
         Only differences between the offsets are meaningful.)"
    );
}

/// Prints the refined wand marker positions and how they differ from the
/// configured ones. Both are in the configured wand frame.
pub fn print_refined_wand_geometry(reference_points: &[Vector3d], refined_points: &[Vector3d]) {
    println!("Refined wand geometry (mm, wand frame):");
    for i in 0..NUM_WAND_MARKERS {
        let deviation = &refined_points[i] - &reference_points[i];
        println!(
            "  marker {} ({:>6}): configured {}  refined {}  deviation {}  |{:.3}|",
            i, WAND_MARKER_NAMES[i],
            fmt_mm(&reference_points[i]), fmt_mm(&refined_points[i]), fmt_mm(&deviation),
            deviation.norm() * 1000.0
        );
    }

    let dist = |pts: &[Vector3d], a: usize, b: usize| (&pts[a] - &pts[b]).norm() * 1000.0;
    for (a, b, note) in [(0, 2, " (scale reference, kept)"), (0, 1, ""), (1, 2, ""), (1, 3, ""), (0, 3, ""), (2, 3, "")] {
        println!(
            "  distance {:>6}-{:<6}: configured {:9.3}  refined {:9.3}  diff {:+.3} mm{}",
            WAND_MARKER_NAMES[a], WAND_MARKER_NAMES[b],
            dist(reference_points, a, b), dist(refined_points, a, b),
            dist(refined_points, a, b) - dist(reference_points, a, b), note
        );
    }

    // Shape checks relative to the line through the left and middle markers.
    if let Ok(gauge) = WandGauge::from_points(refined_points) {
        let right = gauge.to_canonical(&refined_points[2]);
        let bottom = gauge.to_canonical(&refined_points[3]);
        println!(
            "  right marker distance from the left-middle line: {:.3} mm",
            (right.y() * right.y() + right.z() * right.z()).sqrt() * 1000.0
        );
        println!(
            "  bottom arm angle from perpendicular: {:+.3} deg",
            bottom.x().atan2(-bottom.y()).to_degrees()
        );
    }

    println!("Applying the calibration saves these as the wand config:");
    for p in refined_points {
        println!("  points {{ values: {:.6} values: {:.6} values: {:.6} }}", p.x(), p.y(), p.z());
    }
}
