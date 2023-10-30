use common::iter::PairIter;
use math::geometry::line_segment::LineSegment2;
use math::matrix::{vec2f, Matrix3f, Vector2f};

use crate::canvas::bezier::BezierCurve;
use crate::canvas::curve::Curve;
use crate::canvas::ellipse::Ellipse;
use crate::transforms::transform2f;

// TODO: Increase if we can use more anti-aliasing.
const LINEARIZATION_ERROR_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone)]
pub struct Path {
    sub_paths: Vec<SubPath>,
}

impl Path {
    pub fn new() -> Self {
        Self { sub_paths: vec![] }
    }

    pub fn sub_paths(&self) -> &[SubPath] {
        &self.sub_paths
    }

    /// NOTE: The result of this is only valid under the current transform.
    pub fn linearize(&self, transform: &Matrix3f) -> (Vec<Vector2f>, Vec<usize>) {
        let mut verts = vec![];
        let mut path_starts = vec![];

        for sub_path in self.sub_paths() {
            if sub_path.segments.is_empty() {
                continue;
            }

            path_starts.push(verts.len());

            let mut first = true;
            for segment in &sub_path.segments {
                // The current segment has the same start vertex as the previous segment's end
                // vertex.
                if !first {
                    verts.pop();
                }

                match segment {
                    PathSegment::Line(line) => {
                        verts.push(transform2f(transform, &line.start));
                        verts.push(transform2f(transform, &line.end))
                    }
                    PathSegment::BezierCurve(curve) => {
                        let curve = curve.transform(transform);
                        curve.linearize(LINEARIZATION_ERROR_THRESHOLD, &mut verts);
                    }
                    PathSegment::Ellipse(curve) => {
                        let curve = curve.transform(transform);
                        curve.linearize(LINEARIZATION_ERROR_THRESHOLD, &mut verts);
                    }
                }

                first = false;
            }
        }

        path_starts.push(verts.len());
        (verts, path_starts)
    }

    pub fn stroke(&self, width: f32, transform: &Matrix3f) -> (Vec<Vector2f>, Vec<usize>) {
        let (verts, path_starts) = self.linearize(transform);

        let scale = transform[(0, 0)];
        let width_scaled = width * scale;

        let dash_array = &[]; // &[5.0 * scale, 5.0 * scale];

        let mut stroke_vertices = vec![];
        let mut stroke_path_starts = vec![];

        for (i, j) in path_starts.pair_iter() {
            let dashes = crate::raster::stroke::stroke_split_dashes(&verts[*i..*j], dash_array);

            for dash in dashes {
                let (points, starts) = crate::raster::stroke::stroke_poly(&dash, width_scaled);

                stroke_vertices.extend(points);
                stroke_path_starts.extend(starts);
            }
        }

        (stroke_vertices, stroke_path_starts)
    }

    /// Determines whether or not the result of linearizing the path (returned
    /// by self.linearize or self.stroke) can be reused under a new transform.
    pub fn can_reuse_linearized(
        &self,
        current_transform: &Matrix3f,
        last_transform_inv: &Matrix3f,
    ) -> bool {
        let mut all_lines = true;
        for sub_path in &self.sub_paths {
            for segment in &sub_path.segments {
                if let PathSegment::Line(_) = segment {
                    // Good
                } else {
                    all_lines = false;
                    break;
                }
            }

            if !all_lines {
                break;
            }
        }

        if all_lines {
            return true;
        }

        let mut diff = current_transform * last_transform_inv;

        // Ignore translations
        diff[(0, 2)] = 0.0;
        diff[(1, 2)] = 0.0;

        let mut error = 0.0;
        for i in 0..diff.len() {
            error += diff[i];
        }

        error < 1e-3
    }
}

/// A sub-path is a continuous set of line/curve segments where the last point
/// of the previous segment is the same as the start point of the next segment.
#[derive(Debug, Clone)]
pub struct SubPath {
    pub segments: Vec<PathSegment>,
}

impl SubPath {
    pub fn new() -> Self {
        Self { segments: vec![] }
    }
}

#[derive(Debug, Clone)]
pub enum PathSegment {
    /// NOTE: Could also be implemented as a two point bezier curve.
    Line(LineSegment2<f32>),
    Ellipse(Ellipse),
    BezierCurve(BezierCurve),
}

pub enum PathUsage {
    Fill,
    Stroke { width: f32 },
}

pub struct PathBuilder {
    sub_paths: Vec<SubPath>,
    current_sub_path: Option<(Vector2f, SubPath)>,
    position: Vector2f,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self {
            sub_paths: vec![],
            current_sub_path: None,
            position: Vector2f::zero(),
        }
    }

    pub fn move_to(&mut self, pos: Vector2f) {
        if let Some((_, sub_path)) = self.current_sub_path.take() {
            self.sub_paths.push(sub_path)
        }

        self.position = pos;
    }

    fn get_sub_path(&mut self) -> &mut SubPath {
        &mut self
            .current_sub_path
            .get_or_insert((self.position.clone(), SubPath { segments: vec![] }))
            .1
    }

    pub fn line_to(&mut self, pos: Vector2f) {
        let start = self.position.clone();
        self.get_sub_path()
            .segments
            .push(PathSegment::Line(LineSegment2 {
                start,
                end: pos.clone(),
            }));

        self.position = pos;
    }

    /// NOTE: Final point is the end point of the curve.
    pub fn curve_to(&mut self, pts: &[Vector2f]) {
        assert!(!pts.is_empty());

        if pts.len() == 1 {
            self.line_to(pts[0].clone());
            return;
        }

        let mut all_pts = vec![self.position.clone()];
        all_pts.extend_from_slice(pts);

        self.get_sub_path()
            .segments
            .push(PathSegment::BezierCurve(BezierCurve { points: all_pts }));

        self.position = pts.last().cloned().unwrap();
    }

    pub fn close(&mut self) {
        if let Some((start_pt, mut sub_path)) = self.current_sub_path.take() {
            sub_path.segments.push(PathSegment::Line(LineSegment2 {
                start: self.position.clone(),
                end: start_pt,
            }));
            self.sub_paths.push(sub_path);
        }
    }

    pub fn ellipse(
        &mut self,
        center: Vector2f,
        radius: Vector2f,
        start_angle: f32,
        delta_angle: f32,
    ) {
        // Mainly to push the currently active subpath.
        self.move_to(center.clone());

        self.sub_paths.push(SubPath {
            segments: vec![PathSegment::Ellipse(Ellipse {
                center,
                x_axis: vec2f(radius.x(), 0.),
                y_axis: vec2f(0., radius.y()),
                start_angle,
                delta_angle,
            })],
        });
    }

    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let p = vec2f(x, y);
        let w = vec2f(width, 0.0);
        let h = vec2f(0.0, height);

        self.move_to(p.clone());
        self.line_to(&p + &w);
        self.line_to(&p + &w + &h);
        self.line_to(&p + &h);
        self.close();
    }

    pub fn build(mut self) -> Path {
        if let Some((_, sub_path)) = self.current_sub_path.take() {
            self.sub_paths.push(sub_path);
        }

        Path {
            sub_paths: self.sub_paths,
        }
    }
}
