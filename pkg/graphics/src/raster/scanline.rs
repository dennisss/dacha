use core::cmp::Ordering::Equal;

use common::errors::*;
use math::geometry::bounding_box::BoundingBox;
use math::matrix::Vector2f;

use crate::raster::FillRule;

pub struct ScanLineIterator<YIter> {
    /// List of edges in the polygon sorted by increasing y_min.
    edges: Vec<Edge>,

    /// Index of the first edge in 'edges' with y_min > the current scan line.
    next_edge: usize,

    /// Index of each edge currently relevant for the scan line along with its x
    /// intersection point with the current y-scanline.
    active_edges: Vec<ScanLineIntersection>,

    fill_rule: FillRule,

    y_values: YIter,
}

impl<YIter: Iterator<Item = f32>> ScanLineIterator<YIter> {
    /// Arguments:
    /// - vertices:
    /// - path_starts:
    /// - fill_rule:
    /// - y_values: Iterate over y values which we care about scanning. This
    ///   MUST return values in ascending order.
    pub fn create(
        vertices: &[Vector2f],
        path_starts: &[usize],
        fill_rule: FillRule,
        y_values: YIter,
    ) -> Self {
        // TODO: Must verify path_starts.

        // Extract paths from edges.
        let mut edges = vec![];
        edges.reserve_exact(vertices.len());
        {
            let mut path_i = 0;
            for i in 0..vertices.len() {
                while i >= path_starts[path_i + 1] {
                    path_i += 1;
                }

                let next_i = if i + 1 == path_starts[path_i + 1] {
                    path_starts[path_i]
                } else {
                    i + 1
                };

                let start = vertices[i].clone();
                let end = vertices[next_i].clone();
                if start == end {
                    continue;
                }

                // Prune close to horizontal lines as they may cause numerical precision issues
                // when computing intersects.
                // TODO: Tune this threshold and verify there are no cases where this is
                // problematic?
                if (start.y() - end.y()).abs() < 1e-6 {
                    continue;
                }

                edges.push(Edge { start, end });
            }
        }

        // Sort edges in order of ascending y-min coordinate.
        edges.sort_by(|e1, e2| e1.y_min().partial_cmp(&e2.y_min()).unwrap_or(Equal));

        Self {
            edges,
            next_edge: 0,
            active_edges: vec![],
            fill_rule,
            y_values,
        }
    }

    pub fn next(&mut self) -> Option<(f32, &[ScanLineIntersection])> {
        let y = match self.y_values.next() {
            Some(v) => v,
            None => return None,
        };

        // Add edges which are now below the scanline.
        while self.next_edge < self.edges.len() {
            if self.edges[self.next_edge].y_min() > y {
                break;
            }

            self.active_edges.push(ScanLineIntersection {
                edge_index: self.next_edge,
                x: 0.,
                increment: 0,
            });
            self.next_edge += 1;
        }

        // Fixing all existing active edges.
        let mut i = 0;
        while i < self.active_edges.len() {
            let edge = &self.edges[self.active_edges[i].edge_index];

            // Remove edges that are now above the scan line.
            if y >= edge.y_max() {
                self.active_edges.swap_remove(i);
                continue;
            }

            // Compute the 'x' coordinate at the current 'y' coordinate for this
            // edge.
            let delta = &edge.end - &edge.start;
            let x = (delta.x() / delta.y()) * (y - edge.start.y()) + edge.start.x();
            self.active_edges[i].x = x;

            self.active_edges[i].increment = match self.fill_rule {
                FillRule::NonZero => {
                    if delta.y() > 0.0 {
                        1
                    } else {
                        -1
                    }
                }
                FillRule::EvenOdd => 0,
            };

            i += 1;
        }

        // Sort edges in ascending order of x-intercept.
        self.active_edges
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Equal));

        if self.fill_rule == FillRule::EvenOdd {
            for i in 0..self.active_edges.len() {
                self.active_edges[i].increment = if i % 2 == 0 { 1 } else { -1 };
            }
        }

        Some((y, &self.active_edges))
    }
}

#[derive(Debug)]
pub struct ScanLineIntersection {
    edge_index: usize,

    pub x: f32,

    /// Once a non-zero number of increments have been passed when visiting x
    /// coordinates from left to right, pixels should be filled.
    pub increment: isize,
}

struct Edge {
    start: Vector2f,
    end: Vector2f,
}

impl Edge {
    fn y_min(&self) -> f32 {
        self.start.y().min(self.end.y())
    }

    fn y_max(&self) -> f32 {
        self.start.y().max(self.end.y())
    }
}
