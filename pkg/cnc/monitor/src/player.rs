/*
TODO: Use more Instant rather than SystemTime timestamps in thie file.
*/

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::bytes::Bytes;
use executor::bundle::TaskResultBundle;
use executor::sync::{AsyncRwLock, AsyncVariable};
use executor::{channel, lock};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use file::{LocalFile, LocalPath, LocalPathBuf};
use protobuf::Message;

use crate::change::{ChangeEvent, ChangePublisher};
use crate::config::MachineConfigContainer;
use crate::db::ProtobufDB;
use crate::files::FileReference;
use crate::program::*;
use crate::serial_controller::{SerialController, DEFAULT_COMMAND_TIMEOUT};
use crate::tables::ProgramRunTable;

const MIN_DB_FLUSH_RATE: Duration = Duration::from_secs(30);

/// When waiting for the temperature of a heater to enter some range, it must
/// stay in the range for this amount of time to proceed to the next step.
const TEMPERATURE_HOLD_TIME: Duration = Duration::from_secs(4);

/// When waiting for the temperature of a header to become '>= X', the
/// temperature will only be considered ok if it is also
/// '< X + TEMPERATURE_MAX_OVER_MIN'
const TEMPERATURE_MAX_OVER_MIN: f32 = 10.0;

const TEMPERATURE_MIN_UNDER_MIN: f32 = 0.5;

/// Streams a file containing GCode commands to a machine.
///
/// - When created, the player is initially PAUSED and must be started with
///   play().
/// - Terminal states are ERROR, DONE, STOPPED and imply that all background
///   tasks have completed running.
/// - The ServiceResource resource will only report fatal errors upon failure
///   (e.g. state poisoned).
pub struct Player {
    shared: Arc<Shared>,
    task: TaskResource,
}

impl_resource_passthrough!(Player, task);

struct Shared {
    machine_id: u64,
    machine_config: Arc<AsyncRwLock<MachineConfigContainer>>,
    file: FileReference,
    state: AsyncVariable<State>,
    change_publisher: ChangePublisher,
    db: Arc<ProtobufDB>,

    use_silent_mode: bool,

    // This is equivalent to checking if state.state is a terminal state, but doesn't require
    // locking a mutex.
    terminated: AtomicBool,
}

struct State {
    proto: ProgramRun,
    status_message: Option<String>,
    // ETA information and elapsed time.
}

#[derive(Default)]
struct ParsedLine {
    command_to_send: Option<String>,
    state_update: ProgramRun,
    action: Option<LineAction>,
}

enum LineAction {
    WaitForTemperature {
        axis_name: String,
        min_temperature: f32,
    },
}

impl Player {
    /// Creates a new player instance which is initially paused.
    pub async fn create(
        machine_id: u64,
        machine_config: Arc<AsyncRwLock<MachineConfigContainer>>,
        file: FileReference,
        serial_interface: Arc<SerialController>,
        change_publisher: ChangePublisher,
        db: Arc<ProtobufDB>,
    ) -> Result<Self> {
        let mut now = SystemTime::now();

        let mut state_proto = ProgramRun::default();

        state_proto.set_run_id(now.duration_since(UNIX_EPOCH).unwrap().as_micros() as u64);
        state_proto.set_file_id(file.id());
        state_proto.set_machine_id(machine_id);
        state_proto.set_last_updated(now);

        state_proto.set_status(ProgramRun_PlayerState::PAUSED);
        state_proto.set_start_time(now);
        state_proto.set_last_progress_update(now);

        // TODO: If there are no M73 commands in the file (or we think they are
        // inaccurate, then we need a motion simulation based estimator).

        // Setting the initial time estimate based on the file time.
        let maybe_silent_mode = machine_config.read().await?.silent_mode();
        let mut use_silent_mode = false;
        if maybe_silent_mode && file.proto().program().has_silent_duration() {
            use_silent_mode = true;
            state_proto
                .set_estimated_remaining_time(file.proto().program().silent_duration().clone());
        } else if (file.proto().program().has_normal_duration()) {
            state_proto
                .set_estimated_remaining_time(file.proto().program().normal_duration().clone());
        }

        let shared = Arc::new(Shared {
            machine_id,
            machine_config,
            file,
            use_silent_mode,
            db,
            state: AsyncVariable::new(State {
                status_message: None,
                proto: state_proto,
            }),
            terminated: AtomicBool::new(false),
            change_publisher,
        });

        let task = TaskResource::spawn_interruptable(
            "cnc::Player",
            Self::run(shared.clone(), serial_interface),
        );

        Ok(Self { shared, task })
    }

    pub fn is_terminal_state(state: ProgramRun_PlayerState) -> bool {
        state == ProgramRun_PlayerState::DONE
            || state == ProgramRun_PlayerState::ERROR
            || state == ProgramRun_PlayerState::STOPPED
    }

    pub fn terminated(&self) -> bool {
        // let state = self.shared.state.lock().await?.read_exclusive();
        // Ok(Self::is_terminal_state(state.state))

        // TODO: Also check if the state has been poisoned.

        self.shared
            .terminated
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    pub async fn state_proto(&self) -> Result<ProgramRun> {
        let state = self.shared.state.lock().await?.read_exclusive();

        let mut proto = state.proto.clone();

        if let Some(message) = &state.status_message {
            proto.status_message_mut().set_text(message);
        }

        Self::advance_progress(&mut proto)?;

        // TODO: Advance forward the ETA/progress estimates in time.

        Ok(proto)
    }

    async fn run(shared: Arc<Shared>, serial_interface: Arc<SerialController>) -> Result<()> {
        let mut bundle = TaskResultBundle::new();

        let (reader, chunks) = ChunkedFileReader::create(&shared.file.path()).await?;
        bundle.add("ChunkedFileReader", reader.run());

        let (splitter, lines) = LineSplitter::create(chunks)?;
        bundle.add("LineSplitter", splitter.run());

        bundle.add(
            "CommandLoop",
            Self::run_command_loop(shared.clone(), lines, serial_interface),
        );

        let result = bundle.join().await;

        let final_proto = lock!(state <= shared.state.lock().await?, {
            let now = SystemTime::now();
            state.proto.set_end_time(now);
            state.proto.set_last_updated(now);

            // Finalize the last segment
            // TODO: Deduplicate this logic.
            if let Some(last_seg) = state.proto.playing_segments_mut().last_mut() {
                if !last_seg.has_end_time() {
                    last_seg.set_end_time(now);
                }
            }

            if let Err(e) = result {
                eprintln!("Player failed: {}", e);
                state.status_message = Some(e.to_string());
                state.proto.set_status(ProgramRun_PlayerState::ERROR);
            } else if state.proto.status() == ProgramRun_PlayerState::STOPPING {
                state.proto.set_status(ProgramRun_PlayerState::STOPPED);
            } else {
                state.proto.set_status(ProgramRun_PlayerState::DONE);
            }

            state.proto.clone()
        });

        shared
            .terminated
            .store(true, std::sync::atomic::Ordering::SeqCst);

        shared.db.insert::<ProgramRunTable>(&final_proto).await?;

        Self::publish_change(&shared);

        Ok(())
    }

    // TODO: Block manual controls while the player is running.

    async fn run_command_loop(
        shared: Arc<Shared>,
        lines: channel::Receiver<Option<Bytes>>,
        serial_interface: Arc<SerialController>,
    ) -> Result<()> {
        /*
        In grbl, jog cancels would also be helpful.
        */

        // TODO: Need to explicitly turn on/off silent mode somewhere.

        let mut parse_error = false;
        let mut stopping = false;
        let mut current_action = None;
        let mut first_stable_time = None;
        let mut status_message = None;

        // TODO: Throttle this loop
        loop {
            //

            {
                let mut state = shared.state.lock().await?.enter();

                let mut state_changed = false;

                if state.status_message != status_message {
                    state.status_message = status_message.clone();
                    state_changed = true;
                }

                if state.proto.status() == ProgramRun_PlayerState::PAUSING {
                    Self::advance_progress(&mut state.proto)?;
                    state.proto.set_status(ProgramRun_PlayerState::PAUSED);
                    state_changed = true;
                }

                if state.proto.status() == ProgramRun_PlayerState::STARTING {
                    state.proto.set_status(ProgramRun_PlayerState::PLAYING);
                    state.proto.set_last_progress_update(SystemTime::now());
                    state_changed = true;
                }

                if state.proto.status() == ProgramRun_PlayerState::PLAYING {
                    let need_new_segment = match state.proto.playing_segments().last() {
                        Some(seg) => seg.has_end_time(),
                        None => true,
                    };

                    if need_new_segment {
                        let line_num = state.proto.line_number();
                        let seg = state.proto.new_playing_segments();
                        seg.set_start_line(line_num);
                        seg.set_start_time(SystemTime::now());
                        state_changed = true;
                    }
                } else {
                    if let Some(last_seg) = state.proto.playing_segments_mut().last_mut() {
                        if !last_seg.has_end_time() {
                            last_seg.set_end_time(SystemTime::now());
                            state_changed = true;
                        }
                    }
                }

                if state_changed
                    || (state.proto.status() == ProgramRun_PlayerState::PLAYING
                        && SystemTime::now()
                            > SystemTime::from(state.proto.last_updated()) + MIN_DB_FLUSH_RATE)
                {
                    state.proto.set_last_updated(SystemTime::now());

                    let new_proto = state.proto.clone();
                    state.exit();

                    shared.db.insert::<ProgramRunTable>(&new_proto).await?;

                    Self::publish_change(&shared);

                    continue;
                }

                match state.proto.status() {
                    ProgramRun_PlayerState::PLAYING => {
                        // Handled below
                    }
                    ProgramRun_PlayerState::PAUSED => {
                        state.wait().await;
                        continue;
                    }
                    ProgramRun_PlayerState::STOPPING => {
                        stopping = true;
                    }
                    _ => {
                        return Err(format_err!(
                            "In an unexpected state: {:?}",
                            state.proto.status()
                        ));
                    }
                }

                state.exit();
            }

            if stopping {
                break;
            }

            if let Some(action) = &current_action {
                let mut done = false;
                match action {
                    LineAction::WaitForTemperature {
                        axis_name,
                        min_temperature,
                    } => {
                        let axis_config = {
                            // TODO: Verify that this axis is a header (ideally in the parsing
                            // code). TODO: ^ This should also be
                            // verified in the compatibility check for files (the summary can
                            // contain info on which axes are used as heaters).

                            let config = shared.machine_config.read().await?;
                            config
                                .axes()
                                .iter()
                                .find(|a| a.id() == axis_name)
                                .ok_or_else(|| err_msg("Unknown axis"))?
                                .as_ref()
                                .clone()
                        };

                        status_message = Some(format!(
                            "Waiting for temperature of {} to be >= {:.1}",
                            axis_config.name(),
                            *min_temperature
                        ));

                        let current_value = serial_interface.axis_value(&axis_name).await?;
                        if current_value.data.len() >= 1 {
                            let current_temp = current_value.data[0];

                            if current_temp >= *min_temperature - TEMPERATURE_MIN_UNDER_MIN
                                && current_temp < *min_temperature + TEMPERATURE_MAX_OVER_MIN
                            {
                                let now = Instant::now();
                                let first_stable_time = *first_stable_time.get_or_insert(now);

                                // TODO: Instead look up historical metric data so that we can
                                // parallelize the wait for the heater/
                                if now >= first_stable_time + TEMPERATURE_HOLD_TIME {
                                    done = true;
                                }
                            } else {
                                first_stable_time = None;
                            }
                        }
                    }
                }

                if done {
                    current_action = None;
                    first_stable_time = None;
                    status_message = None;
                } else {
                    // TODO: This will delay the setting of the status message in the state which we
                    // want to avoid (for the first round of waiting on the action).
                    executor::sleep(Duration::from_millis(100)).await?;
                    continue;
                }
            }

            let line = match lines.recv().await {
                Ok(Some(v)) => v,
                Ok(None) => break,
                Err(_) => {
                    // This case means that the input preprocessing failed.
                    // This should be pretty rare since it is all basic I/O.
                    return Err(err_msg("Exiting command loop since inputs failed"));
                }
            };

            let mut parsed_line = ParsedLine::default();

            // TODO: If we can't parse it, we will pause the program and require the user to
            // ignore the line explicitly.
            if let Err(e) = Self::parse_line(&shared, &line, &mut parsed_line) {
                eprintln!("Failed to parse gcode: {}", e);
                parse_error = true;
                break;
            }

            if let Some(cmd) = parsed_line.command_to_send {
                serial_interface
                    .send_command(cmd, DEFAULT_COMMAND_TIMEOUT)
                    .await?;
            }

            current_action = parsed_line.action;

            lock!(state <= shared.state.lock().await?, {
                let num = state.proto.line_number() + 1;
                state.proto.set_line_number(num);

                state.proto.merge_from(&parsed_line.state_update)?;

                Ok::<_, Error>(())
            })?;

            // TODO: If we see an un-recognized line, then we should enter an
            // error state and attempt to stop ourselves.
        }

        // TODO: Handle the final value of 'read_line' (so that we set line_number to
        // the final value at the end of the file).

        // TODO:
        // Wait for current moves to finish.
        // Turn off all heaters/etc.

        /////

        // If we are here, then we finished executing all the lines.

        // TODO: Send 'M400\n' to wait for all moves to finish
        // (GRBL doesn't support this though and will return ok once commands
        // are completed).

        // TODO: Move out of the way of the print.

        // ^ May need 2 commands: https://groups.google.com/g/openpnp/c/X3tj8LStGvU

        // Or use 'G4P0' command: https://groups.google.com/g/openpnp/c/EcA5NqzT9BI

        if parse_error {
            return Err(err_msg("Failed to parse line in program."));
        }

        Ok(())
    }

    /*
        TODO: Need to handle segmented runs which may have some user or

        TODO: Warn if we ever get a progress update gcode which increases our ETA (after accounting for the amount of time we've been running)
    estimated_end_time = 6;

         */

    fn advance_progress(state_proto: &mut ProgramRun) -> Result<()> {
        // TODO: Do the same for the percentage.

        if state_proto.status() != ProgramRun_PlayerState::PLAYING
            && state_proto.status() != ProgramRun_PlayerState::PAUSING
        {
            return Ok(());
        }

        if !state_proto.has_last_progress_update() {
            return Ok(());
        }

        let last_progress_update = SystemTime::from(state_proto.last_progress_update());

        // let last_progress_update = state_proto.

        // out.state_update.set_last_progress_update(now);
        //             out.state_update
        //
        // .set_estimated_remaining_time(Duration::from_secs_f32(v.to_f32()? * 60.0));

        Ok(())
    }

    fn parse_line(shared: &Shared, line: &[u8], out: &mut ParsedLine) -> Result<()> {
        // M109

        let mut line_builder = gcode::LineBuilder::new();
        {
            let mut parser = gcode::Parser::new();
            let mut iter = parser.iter(&line, true);
            while let Some(event) = iter.next() {
                match event {
                    gcode::Event::Word(w) => line_builder.add_word(w)?,
                    gcode::Event::ParseError(_) => {
                        return Err(err_msg("Failed to parse the gcode line"));
                    }
                    _ => {}
                }
            }
        }

        let line = match line_builder.finish() {
            Some(v) => v,
            None => return Ok(()),
        };

        let cmd = line.command().to_string();

        let now = SystemTime::now();

        match cmd.as_str() {
            // Don't send these to the machine.
            //
            // M862.3 P "MK3S" ; printer model check
            // M862.1 P0.4 ; nozzle diameter check
            // M115 U3.13.2 ; tell printer latest fw version
            "M862.3" | "M862.1" | "M115" => {
                return Ok(());
            }

            "M73" => {
                let progress_key = if shared.use_silent_mode { 'Q' } else { 'P' };
                let time_key = if shared.use_silent_mode { 'S' } else { 'R' };

                if let Some(v) = line.params().get(&time_key) {
                    out.state_update.set_last_progress_update(now);
                    out.state_update
                        .set_estimated_remaining_time(Duration::from_secs_f32(v.to_f32()? * 60.0));
                }

                if let Some(v) = line.params().get(&progress_key) {
                    out.state_update.set_last_progress_update(now);
                    out.state_update.set_progress(v.to_f32()? / 100.0);
                }
            }

            // Set extruder temperature
            "M104" => {}
            // Set extruder temperature and wait.
            "M109" => {
                // TODO: Verify there are no other params.
                let temp = line
                    .params()
                    .get(&'S')
                    .ok_or_else(|| err_msg("M109 requires S parameter"))?;

                let mut new_line = gcode::LineBuilder::new();
                new_line.add_word(gcode::Word {
                    key: 'M',
                    value: gcode::WordValue::RealValue(104.into()),
                })?;
                new_line.add_word(gcode::Word {
                    key: 'S',
                    value: temp.clone(),
                })?;
                out.command_to_send = Some(new_line.finish().unwrap().to_string_compact());

                // TODO: Verify this axis exists.
                out.action = Some(LineAction::WaitForTemperature {
                    axis_name: "T".into(),
                    min_temperature: temp.to_f32()?,
                });

                // Don't send the regular command.
                return Ok(());
            }

            // Set bed temperature
            "M140" => {}
            // Set bed temperature and wait.
            // TODO: Dedup this code with M109
            "M190" => {
                // TODO: Verify there are no other params.
                let temp = line
                    .params()
                    .get(&'S')
                    .ok_or_else(|| err_msg("M109 requires S parameter"))?;

                let mut new_line = gcode::LineBuilder::new();
                new_line.add_word(gcode::Word {
                    key: 'M',
                    value: gcode::WordValue::RealValue(140.into()),
                })?;
                new_line.add_word(gcode::Word {
                    key: 'S',
                    value: temp.clone(),
                })?;
                out.command_to_send = Some(new_line.finish().unwrap().to_string_compact());

                // TODO: Verify this axis exists.
                out.action = Some(LineAction::WaitForTemperature {
                    axis_name: "B".into(),
                    min_temperature: temp.to_f32()?,
                });

                // Don't send the regular command.
                return Ok(());
            }

            _ => {}
        }

        out.command_to_send = Some(line.to_string_compact());
        Ok(())
    }

    fn publish_change(shared: &Shared) {
        shared.change_publisher.publish(ChangeEvent::new(
            EntityType::MACHINE,
            Some(shared.machine_id),
            false,
        ));
    }

    /// CANCEL SAFE
    pub async fn play(&self) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            if state.proto.status() != ProgramRun_PlayerState::PAUSED {
                return Err(
                    rpc::Status::failed_precondition("Player not currently paused.").into(),
                );
            }

            state.proto.set_status(ProgramRun_PlayerState::STARTING);
            state.notify_all();

            Ok::<_, Error>(())
        })?;

        Self::publish_change(&self.shared);

        Ok(())
    }

    /// CANCEL SAFE
    pub async fn pause(&self) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            if state.proto.status() != ProgramRun_PlayerState::PLAYING {
                return Err(
                    rpc::Status::failed_precondition("Player not currently playing.").into(),
                );
            }

            state.proto.set_status(ProgramRun_PlayerState::PAUSING);
            state.notify_all();

            Ok::<_, Error>(())
        })?;

        Self::publish_change(&self.shared);

        Ok(())
    }

    /// CANCEL SAFE
    pub async fn stop(&self) -> Result<()> {
        lock!(state <= self.shared.state.lock().await?, {
            if state.proto.status() != ProgramRun_PlayerState::PLAYING
                && state.proto.status() != ProgramRun_PlayerState::PAUSED
            {
                return Err(rpc::Status::failed_precondition(
                    "Player not currently playing or paused.",
                )
                .into());
            }

            state.proto.set_status(ProgramRun_PlayerState::STOPPING);
            state.notify_all();

            Ok::<_, Error>(())
        })?;

        Self::publish_change(&self.shared);

        Ok(())
    }
}
