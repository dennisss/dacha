// Logic for gcode parsing/preprocessing into final actions that need to be sent
// to the machine by the player.

use std::time::Duration;
use std::sync::Arc;

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::bytes::Bytes;
use executor::channel;
use math::matrix::{VectorXf, vec2f};

use crate::leveling::*;

#[derive(Default, Debug)]
pub struct ParsedLine {
    pub commands_to_send: Vec<String>,
    pub state_update: ProgramRun,
    pub progress_updated: bool,

    /// TODO: Make this a vector.
    pub action: Option<LineAction>,

    pub capture_frame_after_line: bool,

    /// Index of the object which this line is in. < 0 implies this is not an
    /// object.
    pub object: i32,
}

#[derive(Debug)]
pub enum LineAction {
    WaitForTemperature {
        axis_name: String,
        min_temperature: f32,
        min_is_max_temperature: bool,
    },
    BedPreheat,

    // NOTE: Tool changes are performed as an action since some for some machines like the
    // Carvera, the tool change gcode finishes way before the actual end of the tool change so we
    // need custom logic to monitor the tool change.
    ToolChange {
        tool: i32,
    },
    Pause,
}

pub struct PlayerProgramPreprocessor {
    use_silent_mode: bool,
    use_compact_lines: bool,
    lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    output: channel::Sender<Option<ParsedLine>>,
    current_object: i32,

    current_position: VectorXf,
    leveler: Option<Arc<ZGridLeveler>>,
    preview_state: Option<PreviewState>,
}

struct PreviewState {
    preview: ProgramPreviewProto,
    current_layer_idx: usize,
    layer_frame_captured: bool,
}

use file::LocalPath;

impl PlayerProgramPreprocessor {

    pub async fn run_standalone(
        path: &LocalPath,
        preview: Option<ProgramPreviewProto>,
    ) -> Result<()> {
        use math::vecxf;
        use executor::bundle::TaskResultBundle;
        use crate::program::ChunkedFileReader;
        use crate::program::ProgramParserOp;

        let mut bundle = TaskResultBundle::new();

        let (reader, chunks) = ChunkedFileReader::create(path).await?;
        bundle.add("ChunkedFileReader", reader.run());

        let (parser, elements) = ProgramParserOp::new(chunks);
        bundle.add("ProgramParser", parser.run());

        let use_silent_mode = false;
        let use_compact_lines = false;

        let (processor, lines) = PlayerProgramPreprocessor::new(
            use_silent_mode,
            use_compact_lines,
            preview,
            elements,
            vecxf!(0., 0., 0.),
            None,
        );

        bundle.add("PlayerProgramPreprocessor", processor.run());

        let mut i = 0;
        let mut num_frames = 0;
        while let Ok(v) = lines.recv().await {
            //
            i += 1;

            let v = match v {
                Some(v) => v,
                None => continue
            };
            
            if v.action.is_none() && v.commands_to_send.is_empty() {
                continue;
            }

            if v.capture_frame_after_line {
                println!("Frame Line: {}", i);
                num_frames += 1;
            }

            if i < 100 {
                i += 1;
                // println!("{}: {:?}", i, v);
            }
        }

        println!("Num frames: {}", num_frames);

        bundle.join().await
    }

    pub fn new(
        use_silent_mode: bool,
        use_compact_lines: bool,
        preview: Option<ProgramPreviewProto>,
        lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
        current_position: VectorXf,
        leveler: Option<Arc<ZGridLeveler>>,
    ) -> (Self, channel::Receiver<Option<ParsedLine>>) {
        let (sender, receiver) = channel::bounded(20);

        let preview_state = preview.map(|preview| {
            PreviewState {
                preview,
                current_layer_idx: 0,
                layer_frame_captured: false
            }
        });

        let inst = Self {
            use_silent_mode,
            use_compact_lines,
            lines,
            output: sender,
            current_object: -1,
            current_position,
            leveler,
            preview_state
        };

        (inst, receiver)
    }

    pub async fn run(mut self) -> Result<()> {
        let mut current_line = 1;
        loop {
            let mut line = match self.lines.recv().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => return Ok(()),
            };

            // Incremental current layer tracking.
            if let Some(preview) = &mut self.preview_state {
                if preview.current_layer_idx < preview.preview.layers().len() {
                    let layer = &preview.preview.layers()[preview.current_layer_idx];
                    if current_line > layer.end_line() {
                        preview.current_layer_idx += 1;
                        preview.layer_frame_captured = false;
                    }
                }
            }

            let out = self.process_line(line)?;
            self.output.send(Some(out)).await;

            current_line += 1;
        }

        let _ = self.output.send(None).await;

        Ok(())
    }

    /// Attempts to process a single line of elements from the given list. Will
    /// do nothing if a whole line hasn't been parsed yet.
    fn process_line(&mut self, elements: Vec<gcode::ProgramElement>) -> Result<ParsedLine> {
        let mut out = ParsedLine::default();

        let mut out_line = gcode::LineBuilder::default();

        for el in elements {
            let cmd = match el {
                gcode::ProgramElement::Command(cmd) => cmd,
                gcode::ProgramElement::Error(e) => {
                    return Err(e);
                }
                _ => continue,
            };

            match &cmd {
                gcode::Command::PrusaModelName(_)
                | gcode::Command::NozzleDiameter(_)
                | gcode::Command::PrintFirmwareCapabilities(_) => {
                    // Don't send these to the machine.
                    continue;
                }

                gcode::Command::CancelObject(cmd) => {
                    if let Some(idx) = cmd.starting_object_index {
                        self.current_object = idx;
                    }

                    // Don't send these to the machine.
                    continue;
                }

                gcode::Command::DetailedZProbe(cmd) => {
                    if cmd.words.len() == 1
                        && cmd.words[0].key == 'G'
                        && cmd.words[0].value == gcode::WordValue::Empty
                    {
                        // "G29 G" is a bed 'preheat' in prusa firmware (takes several minutes).
                        out.action = Some(LineAction::BedPreheat);
                        continue;
                    }
                }

                gcode::Command::SetBuildPercentage(cmd) => {
                    let progress = if self.use_silent_mode {
                        &cmd.silent_percentage
                    } else {
                        &cmd.normal_percentage
                    };

                    let time = if self.use_silent_mode {
                        &cmd.silent_time_remaining_mins
                    } else {
                        &cmd.normal_time_remaining_mins
                    };

                    if let Some(v) = time {
                        out.progress_updated = true;
                        out.state_update
                            .set_estimated_remaining_time(Duration::from_secs_f32(
                                v.to_f32() * 60.0,
                            ));
                    }

                    if let Some(v) = progress {
                        out.progress_updated = true;
                        out.state_update.set_progress(v.to_f32() / 100.0);
                    }
                }

                gcode::Command::SetExtruderTemperature(_) => {}
                gcode::Command::SetExtruderTemperatureAndWait(cmd) => {
                    // TODO: Verify there are no other params.
                    let temp = cmd
                        .inner
                        .target_temperature
                        .or(cmd.inner.min_temperature)
                        .ok_or_else(|| err_msg("Missing temperature"))?;

                    // Run without the wait
                    out_line.add(&gcode::SetExtruderTemperature {
                        inner: gcode::SetHeaterTemperature {
                            tool: cmd.inner.tool,
                            min_temperature: Some(temp),
                            target_temperature: None,
                        },
                    });

                    // TODO: Verify this axis exists.
                    out.action = Some(LineAction::WaitForTemperature {
                        axis_name: match cmd.inner.tool {
                            Some(v) => format!("T{}", v),
                            None => "T".into(),
                        },
                        min_temperature: temp.to_f32(),
                        min_is_max_temperature: cmd.inner.target_temperature.is_some(),
                    });

                    // Don't send the regular command.
                    continue;
                }

                // TODO: Dedup this code with M109
                gcode::Command::SetBedTemperature(_) => {}
                gcode::Command::SetBedTemperatureAndWaitCommand(cmd) => {
                    // TODO: Verify there are no other params.
                    let temp = cmd
                        .inner
                        .target_temperature
                        .or(cmd.inner.min_temperature)
                        .ok_or_else(|| err_msg("Missing temperature"))?;

                    // Run without the wait
                    out_line.add(&gcode::SetBedTemperature {
                        inner: gcode::SetHeaterTemperature {
                            tool: None,
                            min_temperature: Some(temp),
                            target_temperature: None,
                        },
                    });

                    // TODO: Verify this axis exists.
                    // TODO: NEed to support multiple actions if there are multiple on the same
                    // line.
                    out.action = Some(LineAction::WaitForTemperature {
                        axis_name: "B".into(),
                        min_temperature: temp.to_f32(),
                        min_is_max_temperature: cmd.inner.target_temperature.is_some(),
                    });

                    // Don't send the regular command.
                    continue;
                }

                gcode::Command::Stop(cmd) => {
                    out.action = Some(LineAction::Pause);

                    // Don't send the regular command.
                    continue;
                }

                gcode::Command::ToolChange(cmd) => {
                    out.action = Some(LineAction::ToolChange { tool: cmd.tool });

                    // TODO: Also do this for SelectTool.

                    // Don't send the regular command.
                    continue;
                }

                gcode::Command::RapidMove(gcode::RapidMove { inner }) => {
                    // TODO: THis assumes that the line is empty aside from the move.
                    if let Some(leveler) = &self.leveler {

                        let (moves, next_position) = leveler.rewrite_move(
                            &self.current_position,
                            inner,
                            true
                        );
                        for c in moves {
                            let mut line = gcode::LineBuilder::default();
                            line.add(&gcode::Command::RapidMove(gcode::RapidMove { inner: c }));

                            let cmd = {
                                if self.use_compact_lines {
                                    line.to_string_compact()
                                } else {
                                    line.to_string()
                                }
                            };

                            out.commands_to_send.push(cmd);

                        }

                        self.current_position = next_position;

                        continue;
                    }
                }

                gcode::Command::LinearMove(gcode::LinearMove { inner }) => {

                    // TODO: Merge with the leveler logic.
                    if self.leveler.is_none() {
                        let mut end_position = self.current_position.clone();
                        if let Some(v) = &inner.x {
                            end_position[0] = v.to_f32();
                        }
                        if let Some(v) = &inner.y {
                            end_position[1] = v.to_f32();
                        }
                        if let Some(v) = &inner.z {
                            end_position[2] = v.to_f32();
                        }

                        if let Some(preview) = &mut self.preview_state {
                            if preview.current_layer_idx < preview.preview.layers().len() && !preview.layer_frame_captured {
                                let layer = &preview.preview.layers()[preview.current_layer_idx];
                                if layer.has_camera_capture_point() {

                                    let pt = vec2f(layer.camera_capture_point().x(), layer.camera_capture_point().y());
                                    let pt2 = vec2f(end_position[0], end_position[1]);

                                    if (pt2 - pt).norm() < 0.1 { // 0.1mm
                                        preview.layer_frame_captured = true;
                                        out.capture_frame_after_line = true;
                                    }

                                }
                            }
                        }

                        self.current_position = end_position;
                    }

                    // TODO: THis assumes that the line is empty aside from the move.
                    if let Some(leveler) = &self.leveler {

                        let (moves, next_position) = leveler.rewrite_move(
                            &self.current_position,
                            inner,
                            false
                        );
                        for c in moves {
                            let mut line = gcode::LineBuilder::default();
                            line.add(&gcode::Command::LinearMove(gcode::LinearMove { inner: c }));

                            let cmd = {
                                if self.use_compact_lines {
                                    line.to_string_compact()
                                } else {
                                    line.to_string()
                                }
                            };

                            out.commands_to_send.push(cmd);

                        }

                        self.current_position = next_position;

                        continue;
                    }
                }

                _ => {}
            }

            out_line.add(&cmd);
        }

        if !out_line.is_empty() {
            let cmd = {
                if self.use_compact_lines {
                    out_line.to_string_compact()
                } else {
                    out_line.to_string()
                }
            };

            if !out.commands_to_send.is_empty() {
                return Err(err_msg("More than just movements on the line"));
            }

            // TODO: Don't allow this if we did linear moves.
            out.commands_to_send.push(cmd);
        }

        out.object = self.current_object;

        Ok(out)
    }
}
