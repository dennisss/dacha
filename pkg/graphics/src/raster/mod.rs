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
use crate::raster::scanline::ScanLineIterator;

pub mod canvas;
pub mod canvas_render_loop;
pub mod line;
pub mod plot;
pub mod scanline;
pub mod stroke;
mod utils;

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

// TODO: Will polygon filling assume that coordinates are centered at points?
// - And will this be consistent with basic line filling algorithms?

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FillRule {
    EvenOdd,
    NonZero,
}

/// Definition of a multi-path polygon with data stored separately.
pub struct PolygonRef<'a> {
    /// Vertices used to represent sub-paths of the polygon.
    /// See path_starts for how they are connected.
    pub vertices: &'a [Vector2f],

    /// Start index of each sub-path in vertices.
    ///
    /// - Sub-path i contains vertices 'vertices[path_starts[i]]' to
    ///   'vertices[path_starts[i + 1] - 1]'
    ///   - The final entry in this list is always an empty path with start
    ///     value vertices.len()
    /// - Each vertex in a sub-path is connected via a line to the next in the
    ///   sub-path's sub-list.
    pub path_starts: &'a [usize],

    pub fill_rule: FillRule,
}

impl<'a> PolygonRef<'a> {
    // pub fn contains_point(&self, point: &Vector2f) -> bool {
    //     // TODO: Run fast bbox test.

    //     let mut xs = vec![];
    //     self.scan_line(point.y(), &mut xs);

    //     let mut num = 0;
    //     for (x, n) in &xs {
    //         if *x > point.x() {
    //             break;
    //         }

    //         num += *n;
    //     }

    //     num != 0
    // }
}

/// Scan-line polygon filling algorithm.
/// NOTE: This uses the even-odd rule.
///
/// TODO: Before this is called for a path, verify that it has closed the
/// sub-paths.
pub fn fill_polygon(
    image: &mut Image<u8>,
    vertices: &[Vector2f],
    color: &Color,
    path_starts: &[usize],
    fill_rule: FillRule,
) -> Result<()> {
    let bbox = BoundingBox::compute(vertices).clip(&image.bbox());

    let y_values = ((bbox.min.y().floor() as usize)..((bbox.max.y() + 1.0).floor() as usize))
        .map(|y| (y as f32) + 0.5);

    let mut scan_line_iter = ScanLineIterator::create(vertices, path_starts, fill_rule, y_values)?;

    while let Some((y, x_intercepts)) = scan_line_iter.next() {
        if x_intercepts.is_empty() {
            continue;
        }

        let mut current_winding = 0;
        let mut x_intercepts_idx = 0;

        // TODO: Get a scan line iterator on the Image object with pre-checked bounds to
        // optimize this.
        //
        // TODO: Only need to go from the min to the max in current x array.
        for x in (bbox.min.x().floor() as usize)..((bbox.max.x() + 1.0).floor() as usize) {
            let x = x as f32;

            while x_intercepts_idx < x_intercepts.len()
                && x_intercepts[x_intercepts_idx].x <= x + 0.5
            {
                current_winding += x_intercepts[x_intercepts_idx].increment;
                x_intercepts_idx += 1;
            }

            if current_winding != 0 {
                image.set(y as usize, x as usize, &color);
                //				add_color(image, y as usize, x as usize, &c);
            }
        }

        while x_intercepts_idx < x_intercepts.len() {
            current_winding += x_intercepts[x_intercepts_idx].increment;
            x_intercepts_idx += 1;
        }

        if current_winding != 0 {
            return Err(err_msg("Scan line ends inside of the polygon"));
        }
    }

    Ok(())
}

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
                .cast::<u8>()
                .into();

            image.set(y, x, &color);
        }
    }

    Ok(())
}

pub async fn run() -> Result<()> {
    let mut img = Image::zero(300, 400, Colorspace::RGBA);

    // TODO: By default, what is the opacity?
    for i in img.array.data.iter_mut() {
        *i = 0xff;
    }

    crate::raster::line::bresenham_line(
        &mut img,
        Vector2i::from_slice(&[350, 50]),
        Vector2i::from_slice(&[10, 10]),
        &Color::rgba(0, 0, 0, 1),
    );
    let verts = &[
        Vector2f::from_slice(&[150., 100.]),
        Vector2f::from_slice(&[300., 300.]),
        Vector2f::from_slice(&[100., 250.]),
    ];

    //	fill_polygon(&mut img, verts,
    //				 &Color::from_slice_with_shape(4, 1, &[255, 0, 0, 1]))?;

    let colors = &[
        Color::rgba(255, 0, 0, 1),
        Color::rgba(0, 255, 0, 1),
        Color::rgba(0, 0, 255, 1),
    ];

    fill_triangle(&mut img, verts, colors)?;

    img.show().await?;

    Ok(())
}

/*
    Good error handling:
    - Need co

*/
