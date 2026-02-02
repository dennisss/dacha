
use std::sync::Arc;

use common::errors::*;
use math::matrix::{VectorXf};
use cnc_monitor_proto::cnc::ZGridLevelingRequest;
use cnc::grid::*;

use crate::serial_controller::{SerialController, IDLE_COMMAND_TIMEOUT, DEFAULT_COMMAND_TIMEOUT};



/*
TODOs:
- Ideally we always present absolute position units to the machine.
    - This will fix a lot of bugs with mixing of gcode
- Get rid of workspaces on the machine and make all communication with the machine in machine coordinates
    - (set workspace 1 to have 0,0,0 offset)
- Z leveling needs to apply to all commands and not just those in the player.
    - But actually, it shouldn't apply to stuff like tool changing.
*/


// /// Amount of space around the probed grid area that we will still allow movement in using the grid.
// const ALLOWED_MARGIN: f32 = 0.01;

const MIN_MOVE_SIZE: f32 = 0.1;
const MAX_ERROR: f32 = 0.02;


// TODO: This should be more workspace aware and apply to a specific workspace.
pub struct ZGridLeveler {
    origin: (f32, f32),
    grid_values: GridValues,
}

impl ZGridLeveler {
    pub async fn probe(
        serial: Arc<SerialController>,
        request: &ZGridLevelingRequest,
    ) -> Result<Self> {

        if request.grid_x_count() < 2 || request.grid_y_count() < 2 {
            return Err(err_msg("Need to probe at least 2 points in each direction"));
        }

        let origin = (request.x_origin(), request.y_origin());

        let grid = Grid::create(
            (request.grid_min_x(), request.grid_min_y()),
            (request.grid_max_x(), request.grid_max_y()),
            request.grid_x_count() as usize,
            request.grid_y_count() as usize,
        );

        // Switch to the probing tool.
        serial.tool_change(0).await?;

        // Clear any leveling data internally in the machine.
        serial.send_command("M370\n", IDLE_COMMAND_TIMEOUT).await?;

        // Switch to workspace 1
        // NOTE: Should be done after the tool change to ensure that we are operating with zero tool offset (so we don't need to offset our z probed points).
        serial.send_command("G54\n", IDLE_COMMAND_TIMEOUT).await?;

        // Set the offset of workspace 1 ot (0,0,0) so that it is identical to global coordinates.
        serial.send_command("G10 L2 P1 X0 Y0 Z0\n", IDLE_COMMAND_TIMEOUT).await?;

        let probe_feed_rate = 60.0;
        let travel_feed_rate = 400.0;
        let travel_height = 3.0;

        // Go above the first point.
        // NOTE: This will internally switch us to absolute positioning.
        serial.goto(request.x_origin(), request.y_origin(), travel_feed_rate).await?;
        serial.wait_for_idle().await?;

        // G38.2 Z-200 F60

        // Probe origin
        serial.send_command("G38.2 Z-200 F80\n", DEFAULT_COMMAND_TIMEOUT).await?;
        serial.wait_for_idle().await?;
        // TODO: Verify that the probe is hit.

        //                             current_value.data.get().and_then(|v| v.get(0)).cloned()

        // TODO: Read from "PRB"?

        let origin_z = serial.get_current_axis_value("Z").await?.data.get().unwrap()[0];

        let travel_z = origin_z + travel_height;
        serial.goto3(None, None, Some(travel_z), travel_feed_rate).await?;

        let mut z_offsets = vec![];

        for (x, y) in grid.scan_order() {
            serial.goto(x, y, travel_feed_rate).await?;

            // Probe
            serial.send_command("G38.2 Z-200 F80\n", DEFAULT_COMMAND_TIMEOUT).await?;
            serial.wait_for_idle().await?;

            let z_offset = serial.get_current_axis_value("Z").await?.data.get().unwrap()[0];
            z_offsets.push(z_offset);

            // Lift up.
            serial.goto3(None, None, Some(travel_z), travel_feed_rate).await?;
        }

        println!("Offsets: {:?}",  z_offsets);

        let grid_values = GridValues::from_scan_values(grid, &z_offsets)?;



        Ok(Self {
            grid_values,
            origin
        })
    }


    /// All commands should be in absolute coordinates.
    ///
    /// 'start_position' should initially be the current machine position and then the point returned by this function.
    pub fn rewrite_move(
        &self,
        start_position: &VectorXf,
        m: &gcode::Move,
        rapid: bool
    ) -> (Vec<gcode::Move>, VectorXf) {

        let mut end_position = start_position.clone();
        if let Some(v) = &m.x {
            end_position[0] = v.to_f32() + self.origin.0;
        }
        if let Some(v) = &m.y {
            end_position[1] = v.to_f32() + self.origin.1;
        }
        if let Some(v) = &m.z {
            end_position[2] = v.to_f32();
        }        

        // TODO: Add the origin point.

        let step_size = self.grid_values.grid().x_interval().min(
            self.grid_values.grid().y_interval()) / 2.0;

        let points = cnc::rewriting::rewrite_move_z(
            &start_position,
            &end_position,
            rapid,
            |pos| self.grid_values.interpolate_value(pos.x(), pos.y()) + pos.z(),
            &cnc::rewriting::RewriteMoveOptions {
                min_move_size: MIN_MOVE_SIZE,
                max_error: MAX_ERROR,
                step_size
            }
        );

        let mut out = vec![];
        for pt in points {
            // TODO: Also do other axes like 'E' if provided.

            // TODO: Clamp the precision to 4 decimal points.
            // TODO: Compress unchanged coordinates.
            out.push(gcode::Move {
                x: Some(pt[0].into()),
                y: Some(pt[1].into()),
                z: Some(pt[2].into()),
                e: None,
                feed_rate: m.feed_rate.clone()
            });
        }

        (out, end_position)
    }


}