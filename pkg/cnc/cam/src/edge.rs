use std::collections::HashSet;

use base_error::*;
use cam_proto::cnc::CutOutProcessorConfig;
use common::line_builder::LineBuilder;
use gerber::GraphicsObject;
use math::{geometry::half_edge::HalfEdgeStruct, matrix::Vector2f};

#[derive(Clone)]
pub struct EdgeCutMetadata {
    /// Ordered list of points forming the closed outer boundary of the PCB.
    pub outer_edge_path: Vec<Vector2f>,

    /// Indexes of all objects in the outer edge of the PCB.
    pub outer_edge_objects: HashSet<usize>,

    /// Indexes of all objects representing the inner edge of the PCB.
    pub inner_edge_objects: HashSet<usize>,
}

impl EdgeCutMetadata {

    pub fn create(objects: &[gerber::GraphicsObject]) -> Result<Self> {
        let mut lines = vec![];

        for (obj_i, obj) in objects.iter().enumerate() {
            let line = match obj.line.clone() {
                Some(v) => v,
                None => continue,
            };

            lines.push((line, obj_i));
        }

        if lines.len() < 3 {
            return Err(err_msg("Not enough lines to form a closed polygon"));
        }

        let mut edge_polygon = None;

        while !lines.is_empty() {
            let mut polygon = vec![];
            let mut polygon_objs = HashSet::new();
            let mut closed = false;

            let ((s, e), object_i) = lines.pop().unwrap();
            polygon.push(s);
            polygon.push(e);
            polygon_objs.insert(object_i);

            loop {
                let first_point = &polygon[0];
                let last_point = polygon.last().unwrap();

                let mut found_match = false;
                for i in 0..lines.len() {
                    let ((mut s, mut e), object_i) = lines[i].clone();
                    if (last_point - &s).norm() < 0.01 {
                        // Fall through
                    } else if (last_point - &e).norm() < 0.01 {
                        core::mem::swap(&mut s, &mut e);
                    } else {
                        continue;
                    }

                    found_match = true;

                    if (first_point - &e).norm() < 0.01 {
                        closed = true;
                    } else {
                        polygon.push(e);
                    }

                    polygon_objs.insert(object_i);

                    lines.swap_remove(i);

                    break;
                }

                if closed || !found_match {
                    break;
                }
            }

            // TODO: Warn about all the edges that we aren't converting into gcode.
            if !closed {
                continue;
            }

            // TODO: Pick the biggest one if there are multiple.
            if edge_polygon.is_some() {
                return Err(err_msg("Multiple closed paths found in edge cuts"));
            }

            edge_polygon = Some((polygon, polygon_objs));
        }

        let polygon = edge_polygon
            .ok_or_else(|| err_msg("Unable to find a closed polygon for the board edge"))?;

        if polygon.0.len() < 3 {
            return Err(err_msg(
                "Didn't find at least 3 distinct polygon points on the board edge.",
            ));
        }

        let outer_edge_objects = polygon.1;

        let mut inner_edge_objects = HashSet::new();
        for i in 0..objects.len() {
            if !outer_edge_objects.contains(&i) {
                inner_edge_objects.insert(i);
            }
        }

        Ok(Self {
            outer_edge_path: polygon.0,
            outer_edge_objects,
            inner_edge_objects
        })
    }

}
