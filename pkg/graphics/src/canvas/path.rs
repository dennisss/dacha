use std::f32::consts::PI;

use common::iter::PairIter;
use math::geometry::bezier::BezierCurve;
use math::geometry::bounding_box::{BoundingBox, BoundingBoxBuilder};
use math::geometry::curve::Curve2;
use math::geometry::ellipse::Ellipse;
use math::geometry::line_segment::LineSegment2;
use math::geometry::transforms::transform2f;
use math::matrix::{vec2f, Matrix3f, Vector2f};

// TODO: Increase if we can use more anti-aliasing.
pub const LINEARIZATION_ERROR_THRESHOLD: f32 = 0.5;

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

    /// Converts the path to a list of line segments.
    ///
    /// 'max_error' is the maximum allowed difference between the lines and
    /// curves.
    pub fn linearize(&self, max_error: f32) -> (Vec<Vector2f>, Vec<usize>) {
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
                        verts.push(line.start.clone());
                        verts.push(line.end.clone());
                    }
                    PathSegment::BezierCurve(curve) => {
                        curve.linearize(max_error, &mut verts);
                    }
                    PathSegment::Ellipse(curve) => {
                        curve.linearize(max_error, &mut verts);
                    }
                }

                first = false;
            }
        }

        path_starts.push(verts.len());
        (verts, path_starts)
    }

    pub fn stroke(&self, width: f32, max_error: f32) -> (Vec<Vector2f>, Vec<usize>) {
        let (verts, path_starts) = self.linearize(max_error);

        let dash_array = &[]; // &[5.0 * scale, 5.0 * scale];

        let mut stroke_vertices = vec![];
        let mut stroke_path_starts = vec![];

        for (i, j) in path_starts.pair_iter() {
            let dashes = crate::raster::stroke::stroke_split_dashes(&verts[*i..*j], dash_array);

            for dash in dashes {
                let (points, starts) = crate::raster::stroke::stroke_poly(&dash, width);

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

    /// Applies a transformation
    pub fn transform(&mut self, transform: &Matrix3f) {
        for sub_path in &mut self.sub_paths {
            for segment in &mut sub_path.segments {
                match segment {
                    PathSegment::Line(segment) => {
                        segment.start = transform2f(transform, &segment.start);
                        segment.end = transform2f(transform, &segment.end);
                    }
                    PathSegment::Ellipse(ellipse) => {
                        *ellipse = ellipse.transform(transform);
                    }
                    PathSegment::BezierCurve(curve) => {
                        *curve = curve.transform(transform);
                    }
                }
            }
        }
    }

    /// Adds an offset to all points in the path.
    pub fn translate(&mut self, offset: Vector2f) {
        for sub_path in &mut self.sub_paths {
            for segment in &mut sub_path.segments {
                match segment {
                    PathSegment::Line(segment) => {
                        segment.start += &offset;
                        segment.end += &offset;
                    }
                    PathSegment::Ellipse(ellipse) => {
                        ellipse.center += &offset;
                    }
                    PathSegment::BezierCurve(curve) => {
                        for pt in &mut curve.points {
                            *pt += &offset;
                        }
                    }
                }
            }
        }
    }

    /// NOTE: This assumes that the path won't be transformed in the future
    pub fn bbox_to(&self, bbox: &mut BoundingBoxBuilder<typenum::U2>) {
        for sub_path in &self.sub_paths {
            for segment in &sub_path.segments {
                match segment {
                    PathSegment::Line(segment) => {
                        bbox.update(&segment.start);
                        bbox.update(&segment.end);
                    }
                    PathSegment::Ellipse(ellipse) => {
                        bbox.update(&ellipse.evaluate_at_angle(ellipse.start_angle));
                        bbox.update(
                            &ellipse.evaluate_at_angle(ellipse.start_angle + ellipse.delta_angle),
                        );

                        for angle in [0.0, PI / 2.0, PI, 3.0 * PI / 2.0, 2.0 * PI] {
                            if ellipse.contains_angle(angle) {
                                bbox.update(&ellipse.evaluate_at_angle(angle));
                            }
                        }
                    }
                    PathSegment::BezierCurve(curve) => {
                        for pt in &curve.points {
                            bbox.update(pt);
                        }
                    }
                }
            }
        }
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
