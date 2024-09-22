// Logic for gcode parsing/preprocessing into final actions that need to be sent
// to the machine by the player.

use std::time::Duration;

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::bytes::Bytes;
use executor::channel;

#[derive(Default)]
pub struct ParsedLine {
    pub command_to_send: Option<String>,
    pub state_update: ProgramRun,
    pub progress_updated: bool,
    pub action: Option<LineAction>,
}

pub enum LineAction {
    WaitForTemperature {
        axis_name: String,
        min_temperature: f32,
        min_is_max_temperature: bool,
    },
}

pub struct PlayerProgramPreprocessor {
    use_silent_mode: bool,
    lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    output: channel::Sender<Option<ParsedLine>>,
}

impl PlayerProgramPreprocessor {
    pub fn new(
        use_silent_mode: bool,
        lines: channel::Receiver<Option<Vec<gcode::ProgramElement>>>,
    ) -> (Self, channel::Receiver<Option<ParsedLine>>) {
        let (sender, receiver) = channel::bounded(20);

        let inst = Self {
            use_silent_mode,
            lines,
            output: sender,
        };

        (inst, receiver)
    }

    pub async fn run(mut self) -> Result<()> {
        loop {
            let mut line = match self.lines.recv().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => return Ok(()),
            };

            self.output.send(Some(self.process_line(line)?)).await;
        }

        let _ = self.output.send(None).await;

        Ok(())
    }

    /// Attempts to process a single line of elements from the given list. Will
    /// do nothing if a whole line hasn't been parsed yet.
    fn process_line(&self, elements: Vec<gcode::ProgramElement>) -> Result<ParsedLine> {
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
                | gcode::Command::PrintFirmwareCapabilities(_)
                | gcode::Command::CancelObject(_) => {
                    // Don't send these to the machine.
                    continue;
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

                _ => {}
            }

            out_line.add(&cmd);
        }

        if !out_line.is_empty() {
            out.command_to_send = Some(out_line.to_string_compact());
        }

        Ok(out)
    }
}
