use std::collections::HashSet;

use base_error::*;
use cam_proto::cnc::CutOutProcessorConfig;
use common::line_builder::LineBuilder;
use gerber::GraphicsObject;
use math::{geometry::half_edge::HalfEdgeStruct, matrix::Vector2f};

pub struct CutOutProcessorOptions {
    pub config: CutOutProcessorConfig,
    pub max_error: f32,
}

/// Performs cutout all the way through a PCB.
/// - This assumes that there is a connected polygon (formed of lines) that
///   represents the outer edge of the board. The diameter of these lines is
///   ignored and the center line is used as the edge of the board.
/// - All other objects on the edge cut layer are milled out regularly.
pub struct CutOutProcessor {
    options: CutOutProcessorOptions,
}

impl CutOutProcessor {
    pub fn new(options: CutOutProcessorOptions) -> Self {
        Self { options }
    }

    pub fn process(&self, objects: &[gerber::GraphicsObject], out: &mut LineBuilder) -> Result<()> {
        // Getting the ordered list of points forming the closed outer boundary of the
        // PCB.
        let (edge_path, edge_objects) = {
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

            polygon
        };

        // Generate the offset polygon (this also verifies that the above polygon is
        // well formed and doesn't have self intersections).
        let cut_path = {
            // println!("PATH: {:?}", edge_path);

            let mut half_edges = HalfEdgeStruct::<bool>::new();
            half_edges.add_face(true, edge_path.iter().cloned());
            half_edges.repair();
            half_edges.merge_faces();

            // Expect there to be one unbounded face and one inner face.
            if half_edges.faces().count() != 2 {
                return Err(format_err!(
                    "Expected outline to be just one boundary. Got: {}",
                    half_edges.faces().count()
                ));
            }

            half_edges = math::geometry::offsetting::offset_faces(
                &half_edges,
                self.options.config.margin() + (self.options.config.tool_diameter() / 2.0),
                self.options.max_error,
            );

            half_edges.merge_faces();

            let faces = math::geometry::half_edge::FaceDebug::get_all(&half_edges);
            // println!("{:?}", faces);
            if faces.len() != 2 {
                return Err(err_msg("More than 1 face after offsetting boundary"));
            }

            let edge_face = faces
                .iter()
                .find(|f| f.outer_component.is_some())
                .ok_or_else(|| err_msg("Can't find the offset face"))?;

            edge_face.outer_component.as_ref().unwrap().clone()
        };

        if cut_path.len() < 3 {
            return Err(err_msg("Path too short"));
        }

        let other_cut_paths = {
            let mut half_edges = HalfEdgeStruct::<bool>::new();

            for (obj_i, obj) in objects.iter().enumerate() {
                if edge_objects.contains(&obj_i) {
                    continue;
                }

                for path in &obj.paths {
                    // TODO: Handle the fill mode.

                    let (vertices, path_starts) = path.path.linearize(0.05);
                    for i in 0..(path_starts.len() - 1) {
                        let start_i = path_starts[i];
                        let end_i = path_starts[i + 1];
                        half_edges.add_face(true, vertices[start_i..end_i].iter().cloned());
                    }
                }
            }

            half_edges.repair();
            half_edges.merge_faces();

            half_edges = math::geometry::offsetting::offset_faces(
                &half_edges,
                -(self.options.config.tool_diameter() / 2.0),
                self.options.max_error,
            );

            half_edges.merge_faces();

            let mut paths = vec![];
            for face in half_edges.faces() {
                let boundary = match face.outer_component() {
                    Some(v) => v.points(),
                    None => continue,
                };

                paths.push(boundary);
            }

            paths
        };

        out.nl();
        out.add("; Cutout");

        // TODO: Consider moving all of these to after the tool change.
        out.add(format!(
            "G00 Z{} F{}",
            self.options.config.clearance_z(),
            self.options.config.rapid_feedrate()
        ));

        // Change tools.
        out.add(format!("T{} M6", self.options.config.tool_index()));

        // Turn on spindle.
        out.add(format!("M03 S{}", self.options.config.spindle_speed()));

        if self.options.config.cut_depth_z() <= 0.0001
            || self.options.config.depth_per_pass_z() <= 0.0001
        {
            return Err(err_msg(
                "Expected positive non-negative cut depth parameters",
            ));
        }

        // TODO: We need to do path order optimization for other_cut_paths.
        // - For now it is fairly simple since we only do one offset path

        for path in other_cut_paths.iter().chain(std::iter::once(&cut_path)) {
            // Move above the first point.
            out.add(format!("G00 X{:.4} Y{:.4}", path[0].x(), path[0].y()));

            // Run cut passes.
            // NOTE: Each iteration of this loop starts with the machine located at the x/y
            // coordinates of the start point.
            let mut current_z = 0.0;
            while current_z < self.options.config.cut_depth_z() {
                current_z += self.options.config.depth_per_pass_z();
                if current_z > self.options.config.cut_depth_z() {
                    current_z = self.options.config.cut_depth_z();
                }

                // Plunge
                out.add(format!(
                    "G01 Z{} F{}",
                    -current_z,
                    self.options.config.feedrate_z()
                ));

                for i in 0..path.len() {
                    let pt = &path[(i + 1) % path.len()];
                    out.add(format!(
                        "G01 X{:.4} Y{:.4} F{}",
                        pt.x(),
                        pt.y(),
                        self.options.config.feedrate_xy()
                    ));
                }
            }

            // Go up.
            out.add(format!(
                "G01 Z{} F{}",
                self.options.config.travel_z(),
                self.options.config.feedrate_z()
            ));
        }

        // Turn off spindle.
        out.add("M05");

        out.add(format!(
            "G00 Z{} F{}",
            self.options.config.clearance_z(),
            self.options.config.rapid_feedrate()
        ));

        out.nl();

        Ok(())
    }
}
