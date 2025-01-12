use base_error::*;
use cam_proto::cnc::IsolationRoutingProcessorConfig;
use common::line_builder::LineBuilder;
use gerber::GraphicsObject;
use math::{
    geometry::{bounding_box::BoundingBoxBuilder, half_edge::HalfEdgeStruct},
    matrix::Vector2f,
};

use crate::tsp::greedy_edge_route;
use crate::vbit::*;

pub struct IsolationRoutingProcessorOptions {
    pub config: IsolationRoutingProcessorConfig,
    pub max_error: f32,
}

pub struct IsolationRoutingProcessor {
    options: IsolationRoutingProcessorOptions,
}

struct CutPath {
    points: Vec<Vector2f>,
    closed: bool,
}

impl IsolationRoutingProcessor {
    pub fn new(options: IsolationRoutingProcessorOptions) -> Self {
        Self { options }
    }

    pub fn process(&self, objects: &[GraphicsObject], out: &mut LineBuilder) -> Result<()> {
        // TODO: Need to verify that traces are actually isolated after running
        // isolation routing.

        /*
        tool_diameter
        tool_v_angle
        min_cut_depth
        cut_width
        cut_depth
        */

        let mut cut_depth = self.options.config.cut_depth();
        let mut cut_width = self.options.config.cut_width();

        if cut_depth == 0.0 {
            if self.options.config.tool_v_angle() == 0.0
                || cut_width == 0.0
                || self.options.config.tool_diameter() == 0.0
            {
                return Err(err_msg("No cut_depth specified but missing some of fields: tool_v_angle, cut_width, tool_diameter"));
            }

            cut_depth = calculate_engraving_depth(
                self.options.config.tool_v_angle(),
                self.options.config.tool_diameter(),
                cut_width,
            );
        }

        if cut_width == 0.0 {
            // NOTE: cut_depth is only needed here if we need to calculate the width for a
            // v-bit.
            if cut_depth == 0.0 || self.options.config.tool_diameter() == 0.0 {
                return Err(err_msg(
                    "No cut_width specified but missing some of fields: cur_depth, tool_diameter",
                ));
            }

            if self.options.config.tool_v_angle() == 0.0 {
                cut_width = self.options.config.tool_diameter();
            } else {
                cut_width = calculate_engraving_diameter(
                    self.options.config.tool_v_angle(),
                    self.options.config.tool_diameter(),
                    cut_depth,
                );
            }
        }

        if cut_width == 0.0 || cut_depth == 0.0 {
            return Err(err_msg(
                "Unable to calculate desired cut_width or cut_depth",
            ));
        }

        if cut_depth < self.options.config.min_cut_depth() {
            return Err(err_msg("Cut depth is too small to clear layer."));
        }

        let object_edges = {
            let mut half_edges = HalfEdgeStruct::<bool>::new();

            for obj in objects {
                for path in &obj.paths {
                    if let gerber::FillMode::Dark = path.fill {
                        //
                    } else {
                        // TODO:
                        println!("Non dark");
                        continue;
                    }

                    let (vertices, path_starts) = path.path.linearize(self.options.max_error);
                    for i in 0..(path_starts.len() - 1) {
                        let start_i = path_starts[i];
                        let end_i = path_starts[i + 1];
                        half_edges.add_face(true, vertices[start_i..end_i].iter().cloned());
                    }
                }
            }

            half_edges.repair();
            half_edges.merge_faces();
            half_edges
        };

        // Generate all contiguous cut paths.
        let mut cut_paths = vec![];

        let num_passes = {
            if self.options.config.num_passes() > 0 {
                self.options.config.num_passes()
            } else {
                1000
            }
        };

        for i in 0..num_passes {
            let mut offset = cut_width / 2.0;
            if i > 0 {
                offset += (i as f32) * cut_width * (1.0 - self.options.config.overlap_percentage());
            }

            if self.options.config.inverted() {
                offset = -offset;
            }

            println!("Pass offset : {}", offset);

            let mut pass_edges = math::geometry::offsetting::offset_faces(
                &object_edges,
                offset,
                self.options.max_error,
            );

            pass_edges.merge_faces();

            // TODO: Clip based on the edge cuts. This may split face boundaries into
            // individual un-closed paths.

            let mut have_valid_face = false;
            for face in pass_edges.faces() {
                let boundary = match face.outer_component() {
                    Some(v) => v.points(),
                    None => continue,
                };

                // Skipping very small boundaries.
                {
                    let mut bbox = BoundingBoxBuilder::new();
                    for p in &boundary {
                        bbox.update(p);
                    }

                    let bbox = bbox.build();
                    let size = bbox.max - bbox.min;

                    if size.x() < 2.0 * self.options.max_error
                        && size.y() < 2.0 * self.options.max_error
                    {
                        continue;
                    }
                }

                have_valid_face = true;

                cut_paths.push(CutPath {
                    points: boundary.clone(),
                    closed: true,
                });
            }

            if !have_valid_face {
                break;
            }

            // TODO: Normalize the starting point for closed contours as the
            // 'lowest' point (though I think the half edge data structure will
            // already guarantee this?)
        }

        // TODO: Also factor in the cost of going plunging and ascending if the paths
        // aren't adjacent.
        let route = greedy_edge_route(cut_paths.len(), |i, j| {
            (&cut_paths[i].points[0] - &cut_paths[j].points[0]).norm()
        });

        println!("Num cut paths: {}", cut_paths.len());

        out.nl();
        out.add("; Isolation Routing");

        // Go to
        out.add(format!(
            "G00 Z{} F{}",
            self.options.config.clearance_z(),
            self.options.config.rapid_feedrate()
        ));

        // Change tools.
        out.add(format!("T{} M6", self.options.config.tool_index()));

        // TODO: Everywhere we need to limit the max precision of the numbers that we
        // write to the gcode (number of digits).

        // Turn on spindle.
        out.add(format!("M03 S{}", self.options.config.spindle_speed()));

        // If true, the start point of the current path directly connects to the end
        // point of the previous path so we will not need to plunge into the
        // material again to start the path.
        let mut connecting_to_last_path = false;

        for route_i in 0..route.len() {
            let path = &cut_paths[route[route_i]];

            if connecting_to_last_path {
                out.add(format!(
                    "G01 X{:.4} Y{:.4} F{}",
                    path.points[0].x(),
                    path.points[0].y(),
                    self.options.config.feedrate_xy()
                ));
            } else {
                // Move above start point
                out.add(format!(
                    "G00 X{:.4} Y{:.4} F{}",
                    path.points[0].x(),
                    path.points[0].y(),
                    self.options.config.rapid_feedrate()
                ));

                // Plunge
                out.add(format!(
                    "G01 Z{} F{}",
                    -cut_depth,
                    self.options.config.feedrate_z()
                ));
            }

            // Cut the path. Note that this also closes the path back to the start point.
            for i in 0..path.points.len() {
                let j = (i + 1) % path.points.len();

                out.add(format!(
                    "G01 X{:.4} Y{:.4} F{}",
                    path.points[j].x(),
                    path.points[j].y(),
                    self.options.config.feedrate_xy()
                ));
            }

            connecting_to_last_path = false;
            if route_i + 1 < route.len() {
                let next_path = &cut_paths[route[route_i + 1]];

                // NOTE: Assuming the current path is closed here.
                let distance = (&next_path.points[0] - &path.points[0]).norm();

                if distance <= 1.005 * self.options.config.cut_width() {
                    connecting_to_last_path = true;
                }
            }

            if !connecting_to_last_path {
                // Go up.
                out.add(format!(
                    "G01 Z{} F{}",
                    self.options.config.travel_z(),
                    self.options.config.feedrate_z()
                ));
            }
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
