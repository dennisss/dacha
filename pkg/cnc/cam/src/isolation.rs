use base_error::*;
use cam_proto::cnc::{IsolationRoutingProcessorConfig, ArcMotionBuilderConfig};
use common::line_builder::LineBuilder;
use common::loops::*;
use gerber::GraphicsObject;
use math::{
    geometry::{bounding_box::BoundingBoxBuilder, half_edge::HalfEdgeStruct},
    matrix::Vector2f,
};

use crate::tsp::greedy_edge_route;
use crate::vbit::*;
use crate::edge::EdgeCutMetadata;
use crate::arc::*;

pub struct IsolationRoutingProcessorOptions {
    pub config: IsolationRoutingProcessorConfig,
    pub max_error: f32,
    pub mark_edge: bool,
    pub arc_config: ArcMotionBuilderConfig,
}

pub struct IsolationRoutingProcessor {
    options: IsolationRoutingProcessorOptions,
}

#[derive(Clone)]
struct CutPath {
    points: Vec<Vector2f>,
    closed: bool,
}

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub(crate) struct FaceLabel {
    /// True if present on the copper gerber layer.
    pub dark: bool,

    /// True if the face is inside of the outer most edge cut of the board.
    pub inbounds: bool,
}

impl FaceLabel {
    pub fn dark() -> Self {
        Self { dark: true, inbounds: false }
    }

    pub fn inbounds() -> Self {
        Self { dark: false, inbounds: true }
    }
}

impl math::geometry::half_edge::FaceLabel for FaceLabel {
    fn union(&self, other: &Self) -> Self {
        Self {
            dark: self.dark || other.dark,
            inbounds: self.inbounds || other.inbounds
        }
    }
}


impl IsolationRoutingProcessor {
    pub fn new(options: IsolationRoutingProcessorOptions) -> Self {
        Self { options }
    }

    pub fn process(&self, objects: &[GraphicsObject], edge_metadata: &EdgeCutMetadata, out: &mut LineBuilder) -> Result<()> {
        // TODO: Need to verify that traces are actually isolated after running
        // isolation routing.

        // TODO: Any holes in the board don't need explicit isolation.

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
            let mut half_edges = HalfEdgeStruct::<FaceLabel>::new();

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
                        half_edges.add_face(FaceLabel::dark(), vertices[start_i..end_i].iter().cloned());
                    }
                }
            }

            half_edges.repair();
            half_edges.merge_faces();
            half_edges
        };

        // Generate all contiguous cut paths.
        let mut cut_paths = vec![];

        // TODO: Verify this is sufficient to ensure isolation along the edge since our cutout also won't be perfect so there is some risk that distinct regions touching the edge may still end up oerlapping.  
        if self.options.mark_edge {
            cut_paths.push(CutPath {
                points: edge_metadata.outer_edge_path.clone(),
                closed: true,
            });            
        }

        let num_passes = {
            if self.options.config.num_passes() > 0 {
                self.options.config.num_passes()
            } else {
                1000
            }
        };

        let mut arc_metrics = ArcMetrics::new();

        // TODO: All of the isolation passes can usually be parallelized
        for i in 0..num_passes {
            let mut offset = (cut_width / 2.0) - self.options.config.erosion();
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

            pass_edges.add_face(FaceLabel::inbounds(), edge_metadata.outer_edge_path.iter().cloned());

            pass_edges.merge_faces();

            let mut candidate_paths = vec![];

            for face in pass_edges.faces() {
                if !face.label().inbounds || !face.label().dark {
                    continue;
                }

                let outer_component = match face.outer_component() {
                    Some(v) => v,
                    None => continue
                };

                // The code below is similar to calling outer_component.points() expect we need to exclude any
                // edges that touch the edge of the board.

                let mut all_closed = true;
                let mut current_path = vec![];

                let mut current_edge = outer_component.start_edge();

                // TODO: Instead just use the overall number of edges as a bound.
                let n = outer_component.points().len() + 1;

                bounded_loop(n, || {
                    let next_edge = current_edge.next();
                    let next_is_start = next_edge.id() == outer_component.start_id();

                    if current_edge.twin().incident_face().label().inbounds {

                        if current_path.is_empty() {
                            current_path.push(current_edge.origin());
                        }

                        if !next_is_start || !all_closed {
                            current_path.push(next_edge.origin());
                        }

                    } else {
                        all_closed = false;
                        
                        if !current_path.is_empty() {
                            candidate_paths.push(CutPath {
                                points: current_path.clone(),
                                closed: all_closed,
                            });
                            current_path.clear();
                        }
                    }

                    current_edge = next_edge;

                    if next_is_start {
                        if !current_path.is_empty() {
                            candidate_paths.push(CutPath {
                                points: current_path.clone(),
                                closed: all_closed,
                            });
                            current_path.clear();
                        }
                        
                        Ok(Loop::Break)
                    } else {
                        Ok(Loop::Continue)
                    }
                }).unwrap();
            }

            let mut have_valid_face = false;

            for path in candidate_paths {
                // Skipping very small boundaries.
                {
                    let mut bbox = BoundingBoxBuilder::new();
                    for p in &path.points {
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

                let n = self.options.config.multiples().max(1);

                for i in 0..n {
                    cut_paths.push(path.clone());
                }
            }

            if !have_valid_face {
                break;
            }

            // TODO: Normalize the starting point for closed contours as the
            // 'lowest' point (though I think the half edge data structure will
            // already guarantee this?)
        }

        // TODO: Some paths won't be clsoed anymore so factor that in too.

        // TODO: Also factor in the cost of going plunging and ascending if the paths
        // aren't adjacent.
        let route = greedy_edge_route(cut_paths.len(), |i, j| {
            (&cut_paths[i].points[0] - &cut_paths[j].points[0]).norm()
        });

        println!("Num cut paths: {}", cut_paths.len());

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
                    "G1 X{:.3} Y{:.3} F{}",
                    path.points[0].x(),
                    path.points[0].y(),
                    self.options.config.feedrate_xy()
                ));
            } else {
                // Move above start point
                out.add(format!(
                    "G0 X{:.3} Y{:.3} F{}",
                    path.points[0].x(),
                    path.points[0].y(),
                    self.options.config.rapid_feedrate()
                ));

                // Plunge
                out.add(format!(
                    "G01 Z{} F{}",
                    -(cut_depth + self.options.config.cut_compression()),
                    self.options.config.feedrate_z()
                ));
            }

            let mut motion_builder = ArcMotionBuilder::new(
                self.options.arc_config.clone(), path.points[0].clone(), self.options.config.feedrate_xy(), &mut arc_metrics);

            // Cut the path. Note that this also closes the path back to the start point.
            // TODO: Will need to implement non-closed paths.
            let end_i = if path.closed { path.points.len() } else { path.points.len() - 1 };
            for i in 0..end_i {
                let j = (i + 1) % path.points.len();
                motion_builder.move_to(path.points[j].clone(), out);
            }

            motion_builder.finish(out);

            connecting_to_last_path = false;
            if route_i + 1 < route.len() {
                let last_point = &path.points[end_i % path.points.len()];

                let next_path = &cut_paths[route[route_i + 1]];

                let distance = (&next_path.points[0] - last_point).norm();

                if distance <= 1.005 * cut_width {
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

        // arc_metrics.print();

        Ok(())
    }
}
