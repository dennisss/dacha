use common::errors::*;
use common::iter::PairIter;
use image::{Color, Colorspace, Image};
use math::combin::bin_coeff;
use math::matrix::{Matrix3f, Vector2f, Vector2i, Vector3f};

use crate::raster::FillRule;

pub struct Canvas {
    // TODO: Make private.
    pub drawing_buffer: Image<u8>,

    // TODO: Decouple the display buffer as its not very related to the canvas.
    pub display_buffer: Image<u8>,
    viewport_transform: Matrix3f,
    transform: Matrix3f,
    transform_stack: Vec<Matrix3f>,
}

impl Canvas {
    pub fn create(height: usize, width: usize, sampling: usize) -> Self {
        assert!(sampling >= 1);

        Self {
            drawing_buffer: Image::zero(height * sampling, width * sampling, Colorspace::RGB),
            display_buffer: Image::zero(height, width, Colorspace::RGB),
            transform: Matrix3f::identity(),
            viewport_transform: crate::transforms::scale2f(&Vector2f::from_slice(&[
                sampling as f32,
                sampling as f32,
            ])),
            transform_stack: vec![],
        }
    }

    pub fn save(&mut self) {
        self.transform_stack.push(self.transform.clone());
    }

    pub fn restore(&mut self) -> Result<()> {
        self.transform = self
            .transform_stack
            .pop()
            .ok_or_else(|| err_msg("No transforms saved on the stack"))?;
        Ok(())
    }

    pub fn scale(&mut self, x: f32, y: f32) {
        self.transform =
            crate::transforms::scale2f(&Vector2f::from_slice(&[x, y])) * &self.transform;
    }

    pub fn translate(&mut self, x: f32, y: f32) {
        self.transform =
            crate::transforms::translate2f(&Vector2f::from_slice(&[x, y])) * &self.transform;
    }

    /// NOTE: The result of this is only valid under the current transform.
    fn linearize_path(&self, path: &Path) -> (Vec<Vector2f>, Vec<usize>) {
        let transform = &self.viewport_transform * &self.transform;

        let mut verts = vec![];
        let mut path_starts = vec![];

        for sub_path in &path.sub_paths {
            path_starts.push(verts.len());
            for segment in &sub_path.segments {
                match segment {
                    PathSegment::Line(line) => {
                        verts.push(transform2f(&transform, &line.start));
                        verts.push(transform2f(&transform, &line.end))
                    }
                    PathSegment::BezierCurve(curve) => {
                        linearize(curve, &transform, &mut verts);
                    }
                    PathSegment::Arc(curve) => {
                        linearize(curve, &transform, &mut verts);
                    }
                }
            }
        }

        path_starts.push(verts.len());
        (verts, path_starts)
    }

    pub fn fill_path(&mut self, path: &Path, color: &Color) -> Result<()> {
        let (verts, path_starts) = self.linearize_path(path);

        // TODO: Currently this assumes that all subpaths are closed. Possibly we should
        // instead Optimize this more and not use all of the empty line segments
        // between lines.
        crate::raster::fill_polygon(
            &mut self.drawing_buffer,
            &verts,
            color,
            &path_starts,
            FillRule::NonZero,
        )?;
        Ok(())
    }

    /// TODO: Must use the non-zero winding rule for this always.
    /// TODO: This has a lot of redundant computation with fill_path if we ever
    /// want to do both.
    pub fn stroke_path(&mut self, path: &Path, width: f32, color: &Color) -> Result<()> {
        let (verts, path_starts) = self.linearize_path(path);

        let scale = self.viewport_transform[(0, 0)];
        let width_scaled = width * scale;

        let dash_array = &[]; // &[5.0 * scale, 5.0 * scale];

        for (i, j) in path_starts.pair_iter() {
            let dashes = crate::raster::stroke::stroke_split_dashes(&verts[*i..*j], dash_array);

            for dash in dashes {
                let points = crate::raster::stroke::stroke_poly(&dash, width_scaled);
                let starts = &[0, points.len()];

                crate::raster::fill_polygon(
                    &mut self.drawing_buffer,
                    &points,
                    color,
                    starts,
                    FillRule::NonZero,
                )?;
            }
        }

        Ok(())
    }

    pub fn clear() {}
}

fn transform2f(mat: &Matrix3f, p: &Vector2f) -> Vector2f {
    let p3 = mat * Vector3f::from_slice(&[p.x(), p.y(), 1.0]);
    Vector2f::from_slice(&[p3.x(), p3.y()])
}

/// When converting a curve into a set of line segments, the line size at which
/// we will stop subdividing the curve.
/// TODO: Instead threshold based on change in slope.
const LINEARIZATION_ERROR_THRESHOLD: f32 = 2.0;

const LINEARIZATION_MIN_STEP: f32 = 0.005;

fn linearize<C: Curve>(curve: &C, mat: &Matrix3f, pts: &mut Vec<Vector2f>) {
    // TODO: This currently doesn't work with complete circles.

    //    let mut out = vec![];
    //    for t in &[0.0, 1.0] {
    //        out.push((*t, transform2f(mat, &curve.evaluate(*t))));
    //    }
    //
    //    let mut i = 0;
    //    // TODO: Limit number of points by a threshold on
    //    while i < out.len() - 1 {
    //        if (&out[i].1 - &out[i + 1].1).norm() < LINEARIZATION_ERROR_THRESHOLD
    //            || (out[i + 1].0 - out[i].0).abs() < LINEARIZATION_MIN_STEP
    //        {
    //            i += 1;
    //            continue;
    //        }
    //
    //        let tmid = (out[i + 1].0 + out[i].0) / 2.0;
    //        out.insert(i + 1, (tmid, transform2f(mat, &curve.evaluate(tmid))));
    //    }
    //
    //    for (_, pt) in out {
    //        pts.push(pt);
    //    }

    // TODO: Implement adaptive splitting based on an error threshold.
    let steps = 10;
    for t in 0..(steps + 1) {
        let p_t = transform2f(mat, &curve.evaluate((t as f32) / (steps as f32)));
        pts.push(p_t);
    }
}

// Implementing

pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug)]
pub struct Path {
    sub_paths: Vec<SubPath>,
}

impl Path {
    pub fn new() -> Self {
        Self { sub_paths: vec![] }
    }
}

/// A sub-path is a continuous set of line segments where the last point of the
/// previous segment is the same as the start point of the next segment.
#[derive(Debug)]
pub struct SubPath {
    segments: Vec<PathSegment>,
}

impl SubPath {
    pub fn new() -> Self {
        Self { segments: vec![] }
    }
}

#[derive(Debug)]
pub enum PathSegment {
    Line(Line),
    Arc(Arc),
    BezierCurve(BezierCurve),
}

/// NOTE: Could also be implemented as a two point bezier curve.
#[derive(Debug)]
pub struct Line {
    pub start: Vector2f,
    pub end: Vector2f,
}

pub trait Curve {
    fn evaluate(&self, t: f32) -> Vector2f;
}

#[derive(Debug)]
pub struct Arc {
    pub center: Vector2f,
    pub radius: Vector2f,

    /// Angle at t=0 in radians.
    pub start_angle: f32,

    /// Change in angle relative to start_angle at t=1.
    pub delta_angle: f32,
}

impl Curve for Arc {
    fn evaluate(&self, t: f32) -> Vector2f {
        let angle = self.start_angle + (t * self.delta_angle);
        &self.center
            + Vector2f::from_slice(&[angle.cos() * self.radius.x(), angle.sin() * self.radius.y()])
    }
}

#[derive(Debug)]
pub struct BezierCurve {
    pub points: Vec<Vector2f>,
}

impl Curve for BezierCurve {
    fn evaluate(&self, t: f32) -> Vector2f {
        if t == 0.0 {
            return self.points[0].clone();
        } else if t == 1.0 {
            return self.points.last().cloned().unwrap();
        }

        let mut sum = Vector2f::zero();
        let n = self.points.len() - 1;
        for i in 0..self.points.len() {
            let coeff =
                (bin_coeff(n, i) as f32) * (1.0 - t).powi((n - i) as i32) * t.powi(i as i32);
            sum += self.points[i].clone() * coeff;
        }

        sum
    }
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
        self.get_sub_path().segments.push(PathSegment::Line(Line {
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
            sub_path.segments.push(PathSegment::Line(Line {
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
            segments: vec![PathSegment::Arc(Arc {
                center,
                radius,
                start_angle,
                delta_angle,
            })],
        });
    }

    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let p = Vector2f::from_slice(&[x, y]);
        let w = Vector2f::from_slice(&[width, 0.0]);
        let h = Vector2f::from_slice(&[0.0, height]);

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
