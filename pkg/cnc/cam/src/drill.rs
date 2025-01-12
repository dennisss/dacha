use base_error::*;
use cam_proto::cnc::DrillProcessorConfig;
use common::line_builder::LineBuilder;
use math::matrix::vec2f;

use crate::tsp::greedy_edge_route;

pub struct DrillProcessorOptions {
    pub config: DrillProcessorConfig,
}

// TODO: Drilling is just a special case of contour cutout and we probably don't
// need two things for this.

pub struct DrillProcessor {
    options: DrillProcessorOptions,
}

impl DrillProcessor {
    pub fn new(options: DrillProcessorOptions) -> Self {
        Self { options }
    }

    pub fn process(&self, drill_holes: &[gerber::DrillHole], out: &mut LineBuilder) -> Result<()> {
        out.nl();
        out.add("; Drilling");

        out.add(format!(
            "G00 Z{} F{}",
            self.options.config.clearance_z(),
            self.options.config.rapid_feedrate()
        ));

        // Change tools.
        out.add(format!("T{} M6", self.options.config.tool_index()));

        // Turn on spindle.
        out.add(format!("M03 S{}", self.options.config.spindle_speed()));

        let route = greedy_edge_route(drill_holes.len(), |i, j| {
            (vec2f(drill_holes[i].x, drill_holes[i].y) - vec2f(drill_holes[j].x, drill_holes[j].y))
                .norm()
        });

        // NOTE: We are currently ignoring the hole diameter and drilling all holes to
        // the same diameter.
        for i in route {
            let hole = &drill_holes[i];

            // if hole.diameter < self.options.config.

            // Move above the hole.
            out.add(format!(
                "G00 X{:.4} Y{:.4} F{}",
                hole.x,
                hole.y,
                self.options.config.rapid_feedrate()
            ));

            // Drill down
            out.add(format!(
                "G01 Z{} F{}",
                self.options.config.drill_z(),
                self.options.config.feedrate_z()
            ));

            // Go back up.
            out.add(format!("G01 Z{}", self.options.config.travel_z()));
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
