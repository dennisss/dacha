/*
Doing arc interpolation:

- Keep accumulating points while they fit either a linear or arc model

- Arc model:
    Variables:
        - 'x'
        - 'y'
        - 'r'
    For a point:
        Error = sqrt((x_i - x)^2 + (y_i - y)^2) - r

        So need the derivative of this w.r.t. 'x', 'y', 'r'


- In an arc:
    - All points equal distance from a center point
    - (x_i - x)^2 + (y_i - y)^2 = r^2

Usually we will be originally approximating an arc already

Will want to limit to some max radius

Also want some max point distance from the curve (ideally)

Also make number of lookahead poitns configurable.

Assumption is that the first point stays but every other point can move a little bit.


(x_i - x)^2 + (y_i - y)^2 = r^2

(x_i - x)(x_i - x)

x_i^2 - x * x_i - x * x_i + x^2


*/


use std::f32::consts::PI;

use common::line_builder::LineBuilder;
use cam_proto::cnc::ArcMotionBuilderConfig;
use math::matrix::{Vector2f, vec2f};
use math::complex::Complex;

use crate::histogram::Histogram;


pub struct ArcMotionBuilder<'a> {
    config: ArcMotionBuilderConfig,
    
    // Buffered list of points to which we need to move to next.
    // This will always have at least one element with the first element being the current position.
    points: Vec<Vector2f>,
    
    writer: ArcGcodeWriter,

    metrics: &'a mut ArcMetrics,
}

pub struct ArcMetrics {
    path_length_deviation_percent: Histogram,
    point_deviation: Histogram,
    deviation: Histogram,
}

impl ArcMetrics {
    pub fn new() -> Self {
        Self {
            path_length_deviation_percent: Histogram::new(Histogram::uniform_boundaries(0.0, 0.01, 0.1)),
            point_deviation: Histogram::new(Histogram::uniform_boundaries(0.0, 0.01, 0.1)),
            deviation: Histogram::new(Histogram::uniform_boundaries(0.0, 0.01, 0.1)),
        }
    }

    pub fn print(&self) {
        println!("path_length_deviation_percent:");
        self.path_length_deviation_percent.print();

        println!("point_deviation:");
        self.point_deviation.print();

        println!("deviation:");
        self.deviation.print();
    }
}

struct ArcGcodeWriter {
    feedrate: f32,
    feedrate_is_set: bool,
}

struct MatchedArc {
    ccw: bool,
    center: Vector2f,
    
    path_length_deviation_percent: f32,
    point_deviation: f32,
    deviation: f32,
}

impl<'a> ArcMotionBuilder<'a> {
    pub fn new(config: ArcMotionBuilderConfig, start_pos: Vector2f, feedrate: f32, metrics: &'a mut ArcMetrics) -> Self {
        assert!(config.min_points() >= 3);

        Self {
            config,
            points: vec![start_pos],
            writer: ArcGcodeWriter {
                feedrate,
                feedrate_is_set: false
            },
            metrics
        }
    }

    pub fn move_to(&mut self, pos: Vector2f, out: &mut LineBuilder) {
        self.points.push(pos.clone());

        if self.points.len() >= (self.config.min_points() as usize) {

            if Self::test_arc(&self.config, &self.points[..]).is_none() {
                if self.points.len() - 1 >= (self.config.min_points() as usize) {
                    self.points.pop();
                    self.write_arc_move(out);

                    let current_point = self.points.pop().unwrap();
                    self.points.clear();
                    self.points.push(current_point);
                    self.points.push(pos);
                } else {
                    self.writer.write_linear_move(&self.points[1], out);
                    self.points.remove(0);
                }
            }
        }
    }

    pub fn finish(mut self, out: &mut LineBuilder) {
        if self.points.len() >= (self.config.min_points() as usize) {
            self.write_arc_move(out);
            return;
        }

        for i in 1..self.points.len() {
            self.writer.write_linear_move(&self.points[i], out);
        }
    }

    fn write_arc_move(&mut self, out: &mut LineBuilder) {
        let arc = Self::test_arc(&self.config, &self.points).unwrap();

        self.metrics.path_length_deviation_percent.increment(arc.path_length_deviation_percent);
        self.metrics.point_deviation.increment(arc.point_deviation);
        self.metrics.deviation.increment(arc.deviation);

        let center_delta = &arc.center - &self.points[0];

        let end_point = &self.points[self.points.len() - 1];

        self.writer.write_arc_move(end_point.clone(), center_delta, arc.ccw, out);
    }

    fn test_arc(config: &ArcMotionBuilderConfig, points: &[Vector2f]) -> Option<MatchedArc> {
        if points.len() < (config.min_points() as usize) {
            return None;
        }

        // TODO: Support approximating a circle if we have >= 4 points when the start and end points are the same.
        // NOTE: All three of these points are guaranteed to be on the circle.
        let center = {
            // Allow for approximating a complete circle using the start point and two other points.
            let mut last_point_idx = points.len() - 1;
            if (&points[last_point_idx] - &points[0]).norm() <= 0.001 && points.len() >= 4 {
                last_point_idx -= 1;
            }
            
            match circle_from_points(
                points[0].clone(),
                points[(last_point_idx - 1) / 2].clone(),
                points[last_point_idx].clone()
            ) {
                Some(v) => v,
                None => return None
            }
        };

        let radius = (&points[0] - &center).norm();
        if radius > config.max_radius() {
            return None;
        }

        let mut point_angles = vec![];
        for p in points {
            let v = (p - &center);
            point_angles.push(v.y().atan2(v.x())); // [-pi, pi]
        }

        // Determine if we are going clockwise or counterclockwise.
        // - For each point, we compare it to the first point and get two angles:
        //  - The positive (CCW) angle.
        //  - The negative (CW) angle.
        // - Compared to all previous points, either the positive one must be
        //   monotonically increasing or the negative one must be monotically
        //   decreasing.
        //   - If neither, then the movements are going back and forth.

        let mut all_pos = true;
        let mut all_neg = true;

        let mut current_pos_angle = normalize_angle(point_angles[1] - point_angles[0]);
        let mut current_neg_angle = current_pos_angle - 2.0*PI;

        assert!(current_pos_angle >= 0.0);
        assert!(current_neg_angle <= 0.0);

        for i in 2..points.len() {
            let mut pos_angle = normalize_angle(point_angles[i] - point_angles[0]);

            // Mainly if the final point has the same angle as the starting point, we want the final
            // point to have an angle of 2*PI so that we can represent complete circles.
            if pos_angle <= 0.0001 {
                if current_pos_angle >= PI {
                    current_pos_angle = 2.0 * PI;
                } else {
                    current_pos_angle = 0.0;
                }
            }

            let neg_angle = pos_angle - 2.0*PI;

            assert!(pos_angle >= 0.0);
            assert!(neg_angle <= 0.0);

            if pos_angle < current_pos_angle {
                all_pos = false;
            }

            if neg_angle > current_neg_angle {
                all_neg = false;
            }

            current_pos_angle = pos_angle;
            current_neg_angle = neg_angle;
        }

        // Zig-zagging
        if !all_pos && !all_neg {
            return None;
        }

        // Can't be both counterclockwise and clockwise.
        assert!(!all_pos || !all_neg);

        // TODO: Do percent deviation based on the length of each segment

        // Checking deviation from the curve.
        let mut point_deviation = 0.0f32;
        let mut deviation = 0.0f32;
        for i in 1..points.len() {
            let expected_point = &center + vec2f(point_angles[i].cos(), point_angles[i].sin())*radius;
            point_deviation = point_deviation.max((&points[i] - &expected_point).norm());
            if point_deviation > config.max_point_deviation() {
                // println!("max_point_deviation: {}", deviation);
                return None;
            }

            // Guessing that the furthest point from the circle is the midpoint of each segment.
            // TODO: Improve this.
            let midpoint_radius = (((&points[i] + &points[i - 1]) / 2.0) - &center).norm();
            deviation = deviation.max((midpoint_radius - radius).abs());
            if deviation > config.max_deviation() {
                // println!("max_deviation: {}", (midpoint_radius - radius).abs());
                return None;
            }
        }

        // Check path / arc length change
        let new_path_length = normalize_angle(*point_angles.last().unwrap() - point_angles[0]) * radius;

        let mut old_path_length = 0.0;
        for i in 1..points.len() {
            old_path_length += (&points[i] - &points[i - 1]).norm();
        }

        if old_path_length.abs() < 0.001 {
            return None;
        }

        let path_length_deviation_percent = ((new_path_length - old_path_length) / old_path_length).abs();
        if path_length_deviation_percent > config.max_path_length_deviation_percent() {
            // println!("radius: {}", radius);
            // println!("max_path_length_deviation_percent: {}; {}; {}", percent_path_change, new_path_length, old_path_length);
            return None;
        }

        Some(MatchedArc {
            center,
            ccw: all_pos,
            path_length_deviation_percent,
            deviation,
            point_deviation
        })
    }
}

impl ArcGcodeWriter {
    fn next_feedrate_str(&mut self) -> String {
        if self.feedrate_is_set {
            String::new()
        } else {
            self.feedrate_is_set = true;
            format!(" F{}", self.feedrate)
        }
    }

    fn write_linear_move(&mut self, point: &Vector2f, out: &mut LineBuilder) {
        // TODO: Avoid repeating the feed rate if it has already been written.
        out.add(format!(
            "G1 X{:.3} Y{:.3}{}",
            point.x(),
            point.y(),
            self.next_feedrate_str()
        ));
    }

    fn write_arc_move(&mut self, end_point: Vector2f, center_delta: Vector2f, ccw: bool, out: &mut LineBuilder) {
        let code = if ccw { "G3" } else { "G2" };
        out.add(format!(
            "{} X{:.3} Y{:.3} I{:.3} J{:.3}{}",
            code,
            end_point.x(),
            end_point.y(),
            center_delta.x(),
            center_delta.y(),
            self.next_feedrate_str()
        ));
    }
}

fn normalize_angle(mut a: f32) -> f32 {
    while a >= (2.0 * PI) {
        a -= 2.0 * PI;
    }
    while a < 0.0 {
        a += 2.0 * PI;
    }

    a
}

macro_rules! ret_none {
    ($e:expr) => {{
        match $e {
            Some(v) => v,
            None => return None
        }
    }};
}

pub fn circle_from_points(
    p1: Vector2f,
    p2: Vector2f,
    p3: Vector2f,
) -> Option<Vector2f> {
    // Same approach as https://math.stackexchange.com/a/3503338

    let z1 = Complex::new(p1.x(), p1.y());
    let z2 = Complex::new(p2.x(), p2.y());
    let z3 = Complex::new(p3.x(), p3.y());

    let w = (z3 - z1) * ret_none!((z2 - z1).try_inv(0.0001));

    let c = (z2 - z1) * (w - w.abs()*w.abs()) * ret_none!((w - w.conjugate()).try_inv(0.0001)) + z1;

    Some(vec2f(c.real(), c.imag()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arc_test() {
        let mut config = ArcMotionBuilderConfig::default();
        config.set_min_points(3);
        config.set_max_deviation(100.0);
        config.set_max_point_deviation(100.0);
        config.set_max_radius(100.0);
        config.set_max_path_length_deviation_percent(0.5);
        
        let mut points = vec![
            vec2f(4.0, 5.0) + vec2f(0.0, 1.41421356),
            vec2f(4.0, 5.0) + vec2f(-1.0, -1.0),
            vec2f(4.0, 5.0) + vec2f(1.0, -1.0),
        ];

        let (center, ccw) = ArcMotionBuilder::test_arc(&config, &points).unwrap();
        assert!(ccw);

        points.reverse();

        let (center, ccw) = ArcMotionBuilder::test_arc(&config, &points).unwrap();
        assert!(!ccw);

        // println!("{:?}  {:?}", center, ccw);

    }

    #[test]
    fn test_arc_real_example() {
        let mut config = ArcMotionBuilderConfig::default();
        config.set_min_points(3);
        config.set_max_deviation(0.02);
        config.set_max_point_deviation(0.02);
        config.set_max_radius(100.0);
        config.set_max_path_length_deviation_percent(0.02);

        let points = vec![
            vec2f(12.8000, 9.7949),
            vec2f(12.7688, 9.9512),
            vec2f(12.5979, 10.5139),
            vec2f(12.5374, 10.6606),
            vec2f(12.2603, 11.1792),
            vec2f(12.1716, 11.3115),
            vec2f(11.7986, 11.7661),
            vec2f(11.6863, 11.8784),
            vec2f(11.2317, 12.2517),
            vec2f(11.0991, 12.3403),
            vec2f(10.5803, 12.6174),
            vec2f(10.4336, 12.6782),
            vec2f(9.8708, 12.8489),
            vec2f(9.7151, 12.8799),
            vec2f(9.1299, 12.9377),
            vec2f(8.9702, 12.9377),
            vec2f(8.3850, 12.8799),
            vec2f(8.2295, 12.8489),
            vec2f(7.6665, 12.6782),
            vec2f(7.5193, 12.6174),
            vec2f(7.0007, 12.3403),
            vec2f(6.8684, 12.2517),
            vec2f(6.4138, 11.8784),
            vec2f(6.3015, 11.7661),
            vec2f(5.9285, 11.3115),
        ];

        let (center, ccw) = ArcMotionBuilder::test_arc(&config, &points).unwrap();

        println!("{:?} {}", center, ccw)


    }

    #[test]
    fn test_arc_real_example2() {
        let mut config = ArcMotionBuilderConfig::default();
        config.set_min_points(3);
        config.set_max_deviation(0.04);
        config.set_max_point_deviation(0.02);
        config.set_max_radius(100.0);
        config.set_max_path_length_deviation_percent(0.02);

        let points = vec![
            vec2f(32.2070, 28.2869),
            vec2f(32.5935, 28.0288),
            vec2f(33.0500, 27.9380),
            vec2f(33.5066, 28.0288),
            vec2f(33.8931, 28.2869),
            vec2f(34.1514, 28.6736),
            vec2f(34.2422, 29.1301),
            vec2f(34.1514, 29.5864),
            vec2f(33.8931, 29.9731),
            vec2f(33.5066, 30.2312),
            vec2f(33.0500, 30.3220),
            vec2f(32.5935, 30.2312),
            vec2f(32.2070, 29.9731),
            vec2f(31.9487, 29.5864),

        ];

        let (center, ccw) = ArcMotionBuilder::test_arc(&config, &points).unwrap();

        println!("{:?} {}", center, ccw)

    }

    #[test]
    fn circle_from_points_test() {

        let tests = vec![
            (
                vec2f(0.0, 0.0),
                vec2f(2.0, 0.0),
                vec2f(1.0, 1.0),
                Some(vec2f(1.0, 0.0))
            ),
            (
                vec2f(4.0, 5.0) + vec2f(0.0, 1.41421356),
                vec2f(4.0, 5.0) + vec2f(-1.0, -1.0),
                vec2f(4.0, 5.0) + vec2f(1.0, -1.0),
                Some(vec2f(4.0, 5.0))
            ),
            (
                vec2f(0.0, 0.0),
                vec2f(2.0, 0.0),
                vec2f(4.0, 0.0),
                None
            ),
            (
                vec2f(1.0, 1.0),
                vec2f(2.0, 2.0),
                vec2f(3.0, 3.0),
                None
            ),
            (
                vec2f(2.0, 2.0),
                vec2f(2.0, 2.0),
                vec2f(3.0, 3.0),
                None
            ),
        ];

        for (p1, p2, p3, expected_c) in tests {
            let c = circle_from_points(
                p1, p2, p3
            );

            assert_eq!(c.is_some(), expected_c.is_some());

            if let Some(expected_c) = expected_c {
                let c = c.unwrap();
                assert!((&c - &expected_c).norm_squared() < 0.0001, "{:?} ?= {:?}", c, expected_c);
            }

        }
    }

}


