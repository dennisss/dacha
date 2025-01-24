use std::f32::consts::PI;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::async_std::task::current;
use common::ceil_div;
use common::typenum::Pow;
use common::{bytes::Bytes, io::Readable};
use executor::{
    bundle::TaskResultBundle,
    channel::{self, oneshot, spsc},
    sync::SyncMutex,
};
use file::{LocalFile, LocalPath};
use gcode::CommandCodec;
use graphics::canvas::{Canvas, Paint, PathBuilder};
use graphics::transforms::transform2f;
use image::Color;
use image::{format::jpeg::encoder::JPEGEncoder, types::ImageType, Image};
use math::matrix::cwise_binary_ops::{CwiseMax, CwiseMin};
use math::matrix::{Vector2f, Vector3f};

use crate::{round_number, round_number_ref};

/*
- For CNC, color in everything with Z < 0

- Generate one image per tool
- For 3d printing, also do one image per layer
    - Highlight overhang in another color

*/

/// TODO: Base this on the largest tool diameter.
const MARGIN_MM: f32 = 5.0;

const REFINED_MARGIN_MM: f32 = 2.0;

const PIXELS_PER_MM: f32 = 10.0;

/// Maximum number of layers we will try to preview.
const MAX_NUM_LAYERS: usize = 2000;

pub struct ProgramVisualizer {
    machine_config: MachineConfig,
    summary: ProgramSummaryProto,

    lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    output: oneshot::Sender<ProgramPreviewProto>,
    image_output: spsc::Sender<Image<u8>>,

    current_position: Vector3f,
    current_coordinate_system: String,
    current_tool: i32,
    current_line: usize,
    current_layer: Option<CurrentLayer>,

    complete_layers: Vec<ProgramLayer>,
}

struct CurrentLayer {
    proto: ProgramLayer,
    canvas: graphics::raster::canvas::RasterCanvas,
    min_position: Option<Vector3f>,
    max_position: Option<Vector3f>,
}

/*
We can generate visualizations for specific machines.
- Note that if the config changes, then we need to re-compute things though.

-

Obviously the simplest option is to just always re-compute everything.


TODO: Eventually we will want to save things like the tool configs in history reports since that is relevant for analysis.

*/

impl ProgramVisualizer {
    pub fn create(
        machine_config: &MachineConfig,
        program_summary: &ProgramSummaryProto,
        lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    ) -> Result<(
        Self,
        oneshot::Receiver<ProgramPreviewProto>,
        spsc::Receiver<Image<u8>>,
    )> {
        let (sender, receiver) = oneshot::channel();

        let default_tool = {
            if machine_config.tools().num_slots() == 0 {
                0
            } else {
                -1
            }
        };

        let (sender2, receiver2) = spsc::bounded(2);

        let inst = Self {
            machine_config: machine_config.clone(),
            summary: program_summary.clone(),
            lines,
            output: sender,
            image_output: sender2,

            // TODO: Have smarter defaults for this.
            current_tool: default_tool,
            current_coordinate_system: String::new(),
            current_position: Vector3f::default(),
            current_line: 1,
            current_layer: None,
            complete_layers: vec![],
        };

        Ok((inst, receiver, receiver2))
    }

    pub async fn run(mut self) -> Result<()> {
        loop {
            let elements = match self.lines.recv().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => return Ok(()),
            };

            for element in elements {
                let command = match element {
                    gcode::ProgramElement::Command(c) => c,
                    _ => continue,
                };

                // TODO: NEed to track when the spindle is on.

                // TODO: Need to process/simulate more motion related commands like
                // relative/absolute mode changes.

                match &command {
                    gcode::Command::Workspace1Coordinates(_) => {
                        self.current_coordinate_system = "G54".into();
                    }
                    gcode::Command::ToolChange(cmd) => {
                        self.current_tool = cmd.tool;
                    }
                    gcode::Command::SelectTool(cmd) => {
                        self.current_tool = cmd.index;
                    }
                    gcode::Command::ParkTool(cmd) => {
                        self.current_tool = -1;
                    }

                    // TODO: Add arcs
                    gcode::Command::LinearMove(gcode::LinearMove { inner })
                    | gcode::Command::RapidMove(gcode::RapidMove { inner }) => {
                        let old_position = self.current_position.clone();

                        let mut new_position = self.current_position.clone();
                        for (i, value) in [inner.x, inner.y, inner.z].into_iter().enumerate() {
                            if let Some(v) = value {
                                new_position[i] = v.to_f32();
                            }
                        }

                        // NOTE: We assume that all extrusion uses relative units.
                        let extrude_amount = inner.e.map(|v| v.to_f32()).unwrap_or(0.0);

                        self.current_position = new_position.clone();

                        self.draw_linear_motion(
                            old_position.clone(),
                            new_position.clone(),
                            extrude_amount,
                        )
                        .await?;
                    }
                    _ => {}
                }
            }

            if self.current_line % 10000 == 0 {
                println!("{}", self.current_line);
            }

            self.current_line += 1;
        }

        self.finalize_layer().await?;

        let mut proto = ProgramPreviewProto::default();

        for layer in self.complete_layers {
            proto.add_layers(layer);
        }

        let _ = self.output.send(proto);

        Ok(())
    }

    async fn draw_linear_motion(
        &mut self,
        start_position: Vector3f,
        end_position: Vector3f,
        extrude_amount: f32,
    ) -> Result<()> {
        if self.current_tool < 0 {
            return Ok(());
        }

        let tool_config = self
            .machine_config
            .tools()
            .loaded_tools()
            .iter()
            .find(|tool| tool.index() == self.current_tool as u32)
            .ok_or_else(|| {
                format_err!(
                    "No tool loaded in machine with index: {}",
                    self.current_tool
                )
            })?
            .as_ref()
            .clone();

        if tool_config.has_extruder() {
            // TODO: Handle retractions with some per-extruder state of
            if extrude_amount <= 0.001 {
                return Ok(());
            }

            let move_distance = (&end_position - &start_position).norm();
            if move_distance <= 0.001 {
                return Ok(());
            }

            self.create_layer().await?;

            let last_layer_z = self.last_layer_z();

            let layer = self.current_layer.as_mut().unwrap();

            let layer_height = layer.proto.z() - last_layer_z;

            let filament_volume = (tool_config.extruder().filament_diameter() / 2.0).powi(2)
                * PI
                * extrude_amount
                * 1.05;

            let line_width = filament_volume / (layer_height * move_distance);

            Self::draw_line(start_position, end_position, line_width, layer)?;
            layer.proto.set_end_line(self.current_line as u32);
        } else if tool_config.has_milling() {
            if extrude_amount != 0.0 {
                return Err(err_msg("Milling tool should not have extrusion"));
            }

            // TODO: Also allow going back up but cutting using a different tool.
            // TODO: Handle coordinate system changes.
            let is_cut = {
                // NOTE: We can't trivially compare to previous layers since we need to see if
                // material was actually cleared at the same location on the previous layer.

                // if let Some(current_layer) = &self.current_layer {
                //     end_position.z() <= current_layer.proto.z() + 0.01
                // } else {
                end_position.z() < 0.0
                // }
            };

            if !is_cut {
                return Ok(());
            }

            let mut diameter = tool_config.milling().diameter();
            if tool_config.milling().v_angle() != 0.0 {
                diameter = cam::vbit::calculate_engraving_diameter(
                    tool_config.milling().v_angle(),
                    diameter,
                    -1.0 * end_position.z(),
                );
            }

            if diameter < 0.0 {
                return Err(format_err!("Negative diameter: {}", diameter));
            }

            self.create_layer().await?;

            let layer = self.current_layer.as_mut().unwrap();

            Self::draw_line(start_position, end_position, diameter, layer)?;
            layer.proto.set_end_line(self.current_line as u32);
        }

        Ok(())
    }

    fn draw_line(
        start_position: Vector3f,
        end_position: Vector3f,
        diameter: f32,
        layer: &mut CurrentLayer,
    ) -> Result<()> {
        let mut path = PathBuilder::new();
        path.move_to(start_position.block(0, 0).to_owned());
        path.line_to(end_position.block(0, 0).to_owned());

        let paint = Paint::color(Color::hex(0xffffff));

        let r = Vector2f::from_slice(&[diameter / 2.0, diameter / 2.0]);

        let mut p = layer.canvas.create_path_stroke(&path.build(), diameter)?;
        p.draw(&paint, &mut layer.canvas)?;

        // Add a round line endcap.
        {
            let mut path = PathBuilder::new();

            path.ellipse(
                start_position.block(0, 0).to_owned(),
                r.clone(),
                0.0,
                2.0 * PI,
            );
            path.ellipse(
                end_position.block(0, 0).to_owned(),
                r.clone(),
                0.0,
                2.0 * PI,
            );

            let mut p = layer.canvas.create_path_fill(&path.build())?;
            p.draw(&paint, &mut layer.canvas)?;
        }

        for pos in &[start_position, end_position] {
            let mut min_pos = pos.clone();
            min_pos[0] -= diameter / 2.0 + REFINED_MARGIN_MM;
            min_pos[1] -= diameter / 2.0 + REFINED_MARGIN_MM;

            let mut max_pos = pos.clone();
            max_pos[0] += diameter / 2.0 + REFINED_MARGIN_MM;
            max_pos[1] += diameter / 2.0 + REFINED_MARGIN_MM;

            layer.min_position = Some(
                min_pos
                    .clone()
                    .cwise_min(layer.min_position.clone().unwrap_or(min_pos.clone())),
            );

            layer.max_position = Some(
                max_pos
                    .clone()
                    .cwise_max(layer.max_position.clone().unwrap_or(max_pos.clone())),
            );
        }

        Ok(())
    }

    async fn create_layer(&mut self) -> Result<()> {
        // TODO: Step one is to see if we need a new layer or if we should re-use the
        // current one.

        if let Some(last_layer) = &self.current_layer {
            let still_valid = last_layer.proto.coordinate_system()
                == self.current_coordinate_system
                && (last_layer.proto.tool_index() as i32) == self.current_tool
                && (last_layer.proto.z() - self.current_position.z()).abs() < 0.01;

            if still_valid {
                return Ok(());
            }

            self.finalize_layer().await?;
        }

        let bounds = self
            .summary
            .bounds()
            .iter()
            .find(|b| b.coordinate_system() == self.current_coordinate_system)
            .ok_or_else(|| err_msg("Missing bounds for coordinate system"))?;

        let raw_width = (bounds.max_position().x() - bounds.min_position().x()).max(0.0);
        let raw_height = (bounds.max_position().y() - bounds.min_position().y()).max(0.0);

        let full_width = raw_width + 2.0 * MARGIN_MM;
        let full_height = raw_height + 2.0 * MARGIN_MM;

        let pixel_width = (PIXELS_PER_MM * full_width) as usize;
        let pixel_height = (PIXELS_PER_MM * full_height) as usize;

        // NOTE: This will start out as completely black (zero).
        let mut canvas =
            graphics::raster::canvas::RasterCanvas::create_grayscale(pixel_height, pixel_width);

        // mm to pixel unit conversion.
        canvas.translate(
            (MARGIN_MM - bounds.min_position().x()) * PIXELS_PER_MM,
            (pixel_height as f32) - (MARGIN_MM - bounds.min_position().y()) * PIXELS_PER_MM,
        );
        canvas.scale(PIXELS_PER_MM, -1.0 * PIXELS_PER_MM);

        let mut proto = ProgramLayer::default();
        proto.set_start_line(self.current_line as u32);
        proto.set_coordinate_system(self.current_coordinate_system.as_str());
        proto.set_tool_index(self.current_tool as u32);
        proto.set_z(round_number(self.current_position.z()));

        proto.image_mut().set_height(full_height);
        proto.image_mut().set_width(full_width);
        proto
            .image_mut()
            .set_left(bounds.min_position().x() - MARGIN_MM);
        proto
            .image_mut()
            .set_bottom(bounds.min_position().y() - MARGIN_MM);

        // TODO: Add image coordinates and height/width

        self.current_layer = Some(CurrentLayer {
            proto,
            canvas,
            min_position: None,
            max_position: None,
        });

        Ok(())
    }

    fn last_layer_z(&self) -> f32 {
        // NOTE: The assumption is that extruders always extrude up from 0.0 and mills
        // always cut down from 0.0.
        self.complete_layers
            .last()
            .map(|layer| layer.z())
            .unwrap_or(0.0)
    }

    async fn finalize_layer(&mut self) -> Result<()> {
        let mut current_layer = match self.current_layer.take() {
            Some(v) => v,
            None => return Ok(()),
        };

        if self.complete_layers.len() > MAX_NUM_LAYERS {
            return Err(err_msg("Too many layers"));
        }

        // A bunch of logic to crop the image to the exact bounding box used by this
        // specific layer.
        //
        // (this will reduce the size of the non-first layer for prusa machines due to
        // the out of bounds purge line printed before the main print).
        //
        // TODO: Don't do any cropping if the crop is the majority of the original
        // image?
        let image = {
            let min_position = match current_layer.min_position.take() {
                Some(v) => v,
                None => return Ok(()),
            };

            let max_position = match current_layer.max_position.take() {
                Some(v) => v,
                None => return Ok(()),
            };

            let min_pixels = transform2f(
                current_layer.canvas.current_transform(),
                &min_position.block(0, 0).to_owned(),
            );

            let max_pixels = transform2f(
                current_layer.canvas.current_transform(),
                &max_position.block(0, 0).to_owned(),
            );

            // TODO: Verify that we aren't off by one or two pixels with this cropping and
            // translation.

            let mut x = min_pixels.x().floor() as usize;
            let mut y = max_pixels.y().floor() as usize;
            if x < 0 {
                x = 0;
            }
            if y < 0 {
                y = 0;
            }

            // TODO: Avoid a subtract from zero here.
            let mut width = (max_pixels.x().floor() as usize)
                .checked_sub(x)
                .unwrap_or(1);
            let mut height = (min_pixels.y().floor() as usize)
                .checked_sub(y)
                .unwrap_or(1);

            width = core::cmp::min(current_layer.canvas.drawing_buffer.width() - x, width);
            height = core::cmp::min(current_layer.canvas.drawing_buffer.height() - y, height);

            let image = current_layer
                .canvas
                .drawing_buffer
                .crop(y, x, height, width);

            *current_layer.proto.image_mut().left_mut() += (x as f32) / PIXELS_PER_MM;
            *current_layer.proto.image_mut().bottom_mut() +=
                ((current_layer.canvas.drawing_buffer.height() - height - y) as f32)
                    / PIXELS_PER_MM;

            current_layer
                .proto
                .image_mut()
                .set_width((width as f32) / PIXELS_PER_MM);
            current_layer
                .proto
                .image_mut()
                .set_height((height as f32) / PIXELS_PER_MM);

            round_number_ref(current_layer.proto.image_mut().height_mut());
            round_number_ref(current_layer.proto.image_mut().width_mut());
            round_number_ref(current_layer.proto.image_mut().left_mut());
            round_number_ref(current_layer.proto.image_mut().bottom_mut());

            image
        };

        self.image_output.send(image).await?;

        self.complete_layers.push(current_layer.proto);

        Ok(())
    }
}
