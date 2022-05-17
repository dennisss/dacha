use std::cmp::Ordering::Equal;
use std::ops::IndexMut;

use common::errors::*;
use image::{Color, Colorspace, Image};
use math::geometry::bounding_box::BoundingBox;
use math::matrix::storage::{NewStorage, StorageType};
use math::matrix::{
    Dimension, Matrix3f, MatrixBase, Vector, Vector2f, Vector2i, Vector2u, Vector3f, VectorNew,
};

use crate::image_show::ImageShow;

pub mod canvas;
pub mod plot;
pub mod stroke;

fn closed_range(mut s: isize, mut e: isize) -> Box<dyn Iterator<Item = isize>> {
    let iter = (s..(e + 1));
    if e > s {
        Box::new((s..=e))
    } else {
        Box::new((e..=s).rev())
    }
}

fn add_color(image: &mut Image<u8>, y: usize, x: usize, color: &Color) {
    let mut color_old = image.get(y, x);

    let alpha = (color[3] as f32) / 255.0;
    image.set(
        y,
        x,
        &(color_old.cast::<f32>() * (1.0 - alpha) + color.cast::<f32>() * alpha).cast::<u8>(),
    );
}

// Representing a polygon.
// vec<Vector2f>

// Always y, x
// Except when represented in a vector

/*
Implied bounds rules:

struct Apple<T: Clone> {
    value: T
}
impl<T> Apple { } // Imply T: Clone

----

struct Apple<T> {

}

*/

// MatrixNew<>

// TODO: Replace with .in_range()
fn is_between<T: Copy + std::cmp::PartialOrd>(value: T, range: (T, T)) -> bool {
    let mut min = range.0;
    let mut max = range.1;
    if max < min {
        min = range.1;
        max = range.0;
    }

    value >= min && value <= max
}

// TODO: Will polygon filling assume that coordinates are centered at points?
// - And will this be consistent with basic line filling algorithms?

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FillRule {
    EvenOdd,
    NonZero,
}

pub struct PolygonRef<'a> {
    pub vertices: &'a [Vector2f],
    pub path_starts: &'a [usize],
    pub fill_rule: FillRule,
}

impl<'a> PolygonRef<'a> {
    pub fn scan_line(&self, y: f32, xs: &mut Vec<(f32, isize)>) {
        xs.clear();

        let mut path_i = 0;
        for i in 0..self.vertices.len() {
            while i >= self.path_starts[path_i + 1] {
                path_i += 1;
            }

            let start = &self.vertices[i];

            let next_i = if i + 1 == self.path_starts[path_i + 1] {
                self.path_starts[path_i]
            } else {
                i + 1
            };

            let end = &self.vertices[next_i];

            if !is_between(y, (start.y(), end.y())) {
                continue;
            }

            let del = end - start;

            // Skip horizontal lines (or empty lines).
            if del.y().abs() < 1e-5 {
                continue;
            }

            // Compute the 'x' coordinate at the current 'y' coordinate for this
            // edge.
            let x = (del.x() / del.y()) * (y - start.y()) + start.x();

            if !is_between(x, (start.x(), end.x())) {
                continue;
            }

            //            println!("|{} {} -> {} {}|", start.x(), start.y(), end.x(),
            // end.y());

            let num = match self.fill_rule {
                FillRule::NonZero => {
                    if del.y() > 0.0 {
                        1
                    } else {
                        -1
                    }
                }
                FillRule::EvenOdd => 0,
            };

            // Possibly also store (i, next_i)
            xs.push((x, num));
        }

        if xs.len() == 0 {
            //            println!("NONE AT {}", y);
            return;
        }

        xs.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Equal));

        if self.fill_rule == FillRule::EvenOdd {
            for i in 0..xs.len() {
                xs[i].1 = if i % 2 == 0 { 1 } else { -1 };
            }
        }

        // Dedup x values (and sum up the numbers associated with each duplicate)
        {
            // NOTE: This assumes that there is at least one element in the list.
            let mut last_i = 0;
            for i in 1..xs.len() {
                let equal = (xs[i].0 - xs[last_i].0).abs() < 1e-6;
                if equal {
                    xs[last_i].1 += xs[i].1;
                } else {
                    last_i += 1;
                    if last_i != i {
                        xs[last_i] = xs[i];
                    }
                }
            }

            xs.truncate(last_i + 1);
        }
    }

    pub fn contains_point(&self, point: &Vector2f) -> bool {
        // TODO: Run fast bbox test.

        let mut xs = vec![];
        self.scan_line(point.y(), &mut xs);

        let mut num = 0;
        for (x, n) in &xs {
            if *x > point.x() {
                break;
            }

            num += *n;
        }

        num != 0
    }
}

/// Scan-line polygon filling algorithm.
/// NOTE: This uses the even-odd rule.
pub fn fill_polygon(
    image: &mut Image<u8>,
    vertices: &[Vector2f],
    color: &Color,
    path_starts: &[usize],
    fill_rule: FillRule,
) -> Result<()> {
    if vertices.len() < 3 {
        return Err(err_msg("Polygon has too few vertices"));
    }

    // TODO: Must verify path_starts.

    let bbox = BoundingBox::compute(vertices).clip(&image.bbox());

    // List of (x, num) values at each scan line.
    // We will have at least one x value per edge.
    let mut xs: Vec<(f32, isize)> = vec![];
    xs.reserve(vertices.len() - 1);

    for y in (bbox.min.y().floor() as usize)..((bbox.max.y() + 1.0).floor() as usize) {
        // Fraction from [0,1] of the current pixel which is occupied by the
        // polygon in the y direction.
        let y = (y as f32) + 0.5; // TODO: Without this +0.5, things don't work well.

        PolygonRef {
            vertices,
            path_starts,
            fill_rule,
        }
        .scan_line(y, &mut xs);

        if xs.is_empty() {
            continue;
        }

        if xs.len() % 2 != 0 {
            println!("{}", y);
            println!("{:?}", xs);
            return Err(err_msg("Odd number of intersections in polygon line."));
        }

        let mut current_num = 0;
        let mut xs_idx = 0;

        // TODO: Only need to go from the min to the max in current x array.
        for x in (bbox.min.x().floor() as usize)..((bbox.max.x() + 1.0).floor() as usize) {
            let x = x as f32;

            while xs_idx < xs.len() && xs[xs_idx].0 <= x + 0.5 {
                current_num += xs[xs_idx].1;
                xs_idx += 1;
            }

            if current_num != 0 {
                image.set(y as usize, x as usize, &color);
                //				add_color(image, y as usize, x as usize, &c);
            }
        }
    }

    Ok(())
}

// Go

pub fn fill_triangle(
    image: &mut Image<u8>,
    verts: &[Vector2f; 3],
    colors: &[Color; 3],
) -> Result<()> {
    let to3d = |v: Vector2f| Vector3f::from_slice(&[v[0], v[1], 0.0]);

    let is_clockwise = to3d(&verts[2] - &verts[0])
        .cross(&to3d(&verts[1] - &verts[0]))
        .z()
        > 0.0;

    // Matrix for converting to Barycentric coordinates. See
    // https://en.wikipedia.org/wiki/Barycentric_coordinate_system#Conversion_between_barycentric_and_Cartesian_coordinates
    let bary_mat = Matrix3f::from_slice(&[
        verts[0].x(),
        verts[1].x(),
        verts[2].x(),
        verts[0].y(),
        verts[1].y(),
        verts[2].y(),
        1.0,
        1.0,
        1.0,
    ]);

    let bary_inv = bary_mat.inverse();

    let is_topleft_edge = |v1: &Vector2f, v2: &Vector2f| {
        let mut is = v2.y() > v1.y() || (v2.y() == v1.y() && v2.x() > v1.x());
        if !is_clockwise {
            is = !is;
        }
        is
    };

    let e01_topleft = is_topleft_edge(&verts[0], &verts[1]);
    let e12_topleft = is_topleft_edge(&verts[1], &verts[2]);
    let e20_topleft = is_topleft_edge(&verts[2], &verts[0]);

    let bbox = BoundingBox::compute(verts).clip(&image.bbox());

    for y in (bbox.min.y() as usize)..=(bbox.max.y() as usize) {
        for x in (bbox.min.x() as usize)..=(bbox.max.x() as usize) {
            let v = Vector3f::from_slice(&[(x as f32) + 0.5, (y as f32) + 0.5, 1.0]);
            let b = &bary_inv * v;

            // Must be either on an edge or inside the triangle.
            let inside_or_edge = b[0] >= 0.0 && b[1] >= 0.0 && b[2] >= 0.0;
            if !inside_or_edge {
                continue;
            }

            // Must either be inside the triangle, or if on an edge, it must be
            // a top-left edge (to avoid overlapping with other triangles).
            let inside_or_topleft_edge = (b[0] > 1e-5 || e12_topleft)
                && (b[1] > 1e-5 || e20_topleft)
                && (b[2] > 1e-5 || e01_topleft);
            if !inside_or_topleft_edge {
                continue;
            }

            let color = (colors[0].cast::<f32>() * b[0]
                + colors[1].cast::<f32>() * b[1]
                + colors[2].cast::<f32>() * b[2])
                .cast::<u8>();

            image.set(y, x, &color);
        }
    }

    Ok(())
}

/// Draws a line between two integer points.
/// TODO: Should appropriately mix alphas
pub fn bresenham_line(image: &mut Image<u8>, start: Vector2i, end: Vector2i, color: &Color) {
    let dx = end.x() - start.x();
    let dy = end.y() - start.y();

    if dx.abs() >= dy.abs() {
        let derror = ((dy as f32) / (dx as f32)).abs();
        let mut error = 0.0;

        let mut y = start.y();
        for x in closed_range(start.x(), end.x()) {
            image.set(y as usize, x as usize, color);

            error += derror;
            if error >= 0.5 {
                y += dy.signum();
                error -= 1.0;
            }
        }
    } else {
        let derror = ((dx as f32) / (dy as f32)).abs();
        let mut error = 0.0;
        let mut x = start.x();
        for y in closed_range(start.y(), end.y()) {
            image.set(y as usize, x as usize, color);

            error += derror;
            if error >= 0.5 {
                x += dx.signum();
                error -= 1.0;
            }
        }
    }
}

pub async fn run() -> Result<()> {
    let mut img = Image::zero(300, 400, Colorspace::RGBA);

    // TODO: By default, what is the opacity?
    for i in img.array.data.iter_mut() {
        *i = 0xff;
    }

    bresenham_line(
        &mut img,
        Vector2i::from_slice(&[350, 50]),
        Vector2i::from_slice(&[10, 10]),
        &Color::from_slice_with_shape(4, 1, &[0, 0, 0, 1]),
    );
    let verts = &[
        Vector2f::from_slice(&[150., 100.]),
        Vector2f::from_slice(&[300., 300.]),
        Vector2f::from_slice(&[100., 250.]),
    ];

    //	fill_polygon(&mut img, verts,
    //				 &Color::from_slice_with_shape(4, 1, &[255, 0, 0, 1]))?;

    let colors = &[
        Color::from_slice_with_shape(4, 1, &[255, 0, 0, 1]),
        Color::from_slice_with_shape(4, 1, &[0, 255, 0, 1]),
        Color::from_slice_with_shape(4, 1, &[0, 0, 255, 1]),
    ];

    fill_triangle(&mut img, verts, colors)?;

    img.show().await?;

    Ok(())
}

/*
    Good error handling:
    - Need co

*/
