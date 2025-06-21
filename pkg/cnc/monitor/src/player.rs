/*
TODO: Use more Instant rather than SystemTime timestamps in thie file.
*/

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base_error::*;
use base_units::format_duration_secs;
use cnc_monitor_proto::cnc::*;
use common::bit_set::BitSet;
use common::bytes::Bytes;
use common::hash::FastHasherBuilder;
use common::typenum::U20;
use executor::bundle::TaskResultBundle;
use executor::sync::{AsyncRwLock, AsyncVariable};
use executor::{channel, lock};
use executor_multitask::{impl_resource_passthrough, TaskResource};
use file::{LocalFile, LocalPath, LocalPathBuf};
use protobuf::Message;
use db_table::ProtobufDB;

use crate::change::{ChangeEvent, ChangePublisher};
use crate::config::MachineConfigContainer;
use crate::files::FileReference;
use crate::program::*;
use crate::serial_controller::{PendingCommand, SerialController, DEFAULT_COMMAND_TIMEOUT};
use crate::tables::ProgramRunTable;
use crate::player_preprocessor::*;

const MIN_DB_FLUSH_RATE: Duration = Duration::from_secs(30);

/// When waiting for the temperature of a heater to enter some range, it must
/// stay in the range for this amount of time to proceed to the next step.
const TEMPERATURE_HOLD_TIME: Duration = Duration::from_secs(4);

/// When waiting for the temperature of a header to become '>= X', the
/// temperature will only be considered ok if it is also
/// '< X + TEMPERATURE_MAX_OVER_MIN'
const TEMPERATURE_COARSE_MAX_OVER_MIN: f32 = 10.0;

const TEMPERATURE_FINE_MAX_OVER_MIN: f32 = 0.5;

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
    use_compact_lines: bool,
    must_keep_alive: bool,
    max_enqueued_commands: usize,

    // This is equivalent to checking if state.state is a terminal state, but doesn't require
    // locking a mutex.
    terminated: AtomicBool,
}

struct State {
    proto: ProgramRun,
    status_message: Option<String>,
    // ETA information and elapsed time.
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

        // TODO: Ensure the Prusa tool mapper is disabled during printing and emulate
        // that at a higher level.

        let config = machine_config.read().await?;

        // Setting the initial time estimate based on the file time.
        let maybe_silent_mode = config.silent_mode();
        let mut use_silent_mode = false;
        if maybe_silent_mode && file.proto().program().has_silent_duration() {
            use_silent_mode = true;
            state_proto
                .set_estimated_remaining_time(file.proto().program().silent_duration().clone());
        } else if (file.proto().program().has_normal_duration()) {
            state_proto
                .set_estimated_remaining_time(file.proto().program().normal_duration().clone());
        }

        let use_compact_lines = {
            // This is not true everything since most embedded firmwares don't properly
            // handle gcode commands without a minimum amount of whitespace.
            //
            // - M555 in Prusa firmware MUST have whitespaces between words (only for
            //   preview generation).
            // - Tool changes in Carvera firmware MUST have a whitespace between the 'T' and
            //   'M'.
            match config.firmware() {
                MachineConfig_Firmware::MARLIN
                | MachineConfig_Firmware::GRBL
                | MachineConfig_Firmware::PRUSA => true,
                _ => false,
            }
        };

        let must_keep_alive = config.firmware() == MachineConfig_Firmware::PRUSA
            || config.firmware() == MachineConfig_Firmware::MARLIN;

        let max_enqueued_commands =
            core::cmp::max(1, config.serial().max_player_in_flight_commands() as usize);

        drop(config);

        // Initialize objects state.
        // One uncancelled instance for each object.
        state_proto.objects_mut().set_current_object_index(-1);
        for obj in file.proto().program().objects() {
            let out = state_proto.objects_mut().new_objects();
            out.set_index(obj.index());
        }

        let shared = Arc::new(Shared {
            machine_id,
            machine_config,
            file,
            use_silent_mode,
            use_compact_lines,
            must_keep_alive,
            max_enqueued_commands,
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

    pub async fn toggle_object(&self, object_index: u32, cancelled: bool) -> Result<()> {
        let state = self.shared.state.lock().await?;

        lock!(state <= state, {
            if (object_index as usize) >= state.proto.objects().objects().len() {
                return Err(rpc::Status::invalid_argument(format!(
                    "No object with index {}",
                    object_index
                ))
                .into());
            }

            let line_num = state.proto.line_number();

            let obj = &mut state.proto.objects_mut().objects_mut()[object_index as usize];

            if cancelled {
                if !obj.has_cancelled() {
                    obj.cancelled_mut().set_cancel_time(SystemTime::now());
                    obj.cancelled_mut().set_cancel_line(line_num);
                }
            } else {
                if obj.cancelled().skipped_lines() > 0 {
                    return Err(rpc::Status::failed_precondition(
                        "Object already partially skipped so can't be uncancelled",
                    )
                    .into());
                }

                obj.clear_cancelled();
            }

            Ok(())
        })
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

        proto.set_last_updated(SystemTime::now());

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

        let (parser, elements) = ProgramParserOp::new(chunks);
        bundle.add("ProgramParser", parser.run());

        let (processor, lines) = PlayerProgramPreprocessor::new(
            shared.use_silent_mode,
            shared.use_compact_lines,
            elements,
        );
        bundle.add("PlayerProgramPreprocessor", processor.run());

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
        lines: channel::Receiver<Option<ParsedLine>>,
        serial_interface: Arc<SerialController>,
    ) -> Result<()> {
        /*
        In grbl, jog cancels would also be helpful.
        */

        // TODO: Need to explicitly turn on/off silent mode somewhere.

        let mut parser = gcode::ProgramParser::default();
        let mut stopping = false;
        let mut current_action = None;
        let mut first_stable_time = None;
        let mut status_message = None;

        let mut cancelled_objects = HashSet::<u32, FastHasherBuilder>::default();

        // See the other usage of this for why this is here.
        let mut last_alive = Instant::now();
        if shared.must_keep_alive {
            serial_interface
                .send_command("G4 P100\n", DEFAULT_COMMAND_TIMEOUT)
                .await?;
            last_alive = Instant::now();
        }

        let mut received_all_lines = false;
        let mut newly_enqueued_lines = vec![];

        let mut enqueued_commands = VecDeque::<PendingCommand>::new();

        // TODO: Throttle this loop
        while enqueued_commands.len() > 0 || current_action.is_some() || !received_all_lines {
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
                        seg.set_start_line(line_num + 1);
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

                cancelled_objects.clear();
                for object in state.proto.objects().objects() {
                    if object.has_cancelled() {
                        cancelled_objects.insert(object.index());
                    }
                }

                state.exit();
            }

            if stopping {
                break;
            }

            // Try to dequeue any fully sent commands so that we can send more.
            while let Some(cmd) = enqueued_commands.pop_front() {
                // Normally only block for the command if it is ready or if we must either
                // because:
                // - we are out of space
                // - we have no more lines to enqueue
                // - we are about to run an action that depends on these commands being executed
                //   first.
                if !cmd.ready().await
                    && enqueued_commands.len() + 1 < shared.max_enqueued_commands
                    && !received_all_lines
                    && current_action.is_none()
                {
                    enqueued_commands.push_front(cmd);
                    break;
                }

                cmd.wait().await?;
            }

            if let Some(action) = &current_action {
                let mut done = false;
                match action {
                    LineAction::BedPreheat => {
                        // This is equivalent to the Prusa "G29 G" commands which waits in firmware
                        // for a period of time for heat to sufficiently soak in. Since this takes a
                        // while to do while blocking the serial port, we want to process it
                        // ourselves.
                        //
                        // - Firmware side algorithm is: https://github.com/prusa3d/Prusa-Firmware-Buddy/blob/9b5764d00146119896c43e7b5f4d5293dde6cf42/lib/Marlin/Marlin/src/feature/bed_preheat.cpp#L44
                        // - The asumption is that during the preheat time, the firmware will be
                        //   resetting the stepper timeout so the steppers won't timeout during our
                        //   preheat.

                        let bed_data = serial_interface.get_current_axis_value("B").await?;
                        let data = bed_data
                            .data
                            .get()
                            .ok_or_else(|| err_msg("Missing bed temp data"))?;
                        if data.len() != 2 {
                            return Err(err_msg("Insufficient bed temp data"));
                        }

                        let data_timestamp = bed_data.data.last_updated().unwrap();

                        if data[1] < 60.0 {
                            // Temperature too low for preheating.
                            done = true;
                        } else {
                            let is_stable = (data[0] - data[1]).abs() <= 15.0;

                            if is_stable {
                                // TODO: Ideally look through the metric history and find to find
                                // the min temperature in the last N
                                // seconds.
                                let start_time = *first_stable_time.get_or_insert(data_timestamp);

                                let preheat_time = Duration::from_secs(
                                    180 + ((data[1] as u64) - 60) * ((12 * 60) / 50),
                                );
                                let elapsed_time = Instant::now().duration_since(start_time);

                                if elapsed_time >= preheat_time {
                                    done = true;
                                } else {
                                    status_message = Some(format!(
                                        "Bed Preheat: Waiting for {}",
                                        format_duration_secs(preheat_time - elapsed_time)
                                    ))
                                }
                            } else {
                                first_stable_time = None;
                                status_message =
                                    Some("Bed Preheat: Waiting for bed to heat up...".into());
                            }
                        }
                    }

                    LineAction::WaitForTemperature {
                        axis_name,
                        min_temperature,
                        min_is_max_temperature,
                    } => {
                        let axis_config = {
                            // TODO: Verify that this axis is a header (ideally in the parsing
                            // code). TODO: ^ This should also be
                            // verified in the compatibility check for files (the summary can
                            // contain info on which axes are used as heaters).

                            let config = shared.machine_config.read().await?;
                            config
                                .axes_map()
                                .get(axis_name)
                                .ok_or_else(|| err_msg("Unknown axis"))?
                                .clone()
                        };

                        status_message = Some(format!(
                            "Waiting for temperature of {} to be {} {:.1}",
                            axis_config.name(),
                            if *min_is_max_temperature { "==" } else { ">=" },
                            *min_temperature,
                        ));

                        // TODO: Need to get a recent value.
                        let current_value = serial_interface.axis_value(&axis_name).await?;

                        // current_value.data.last_updated()
                        if let Some(current_temp) =
                            current_value.data.get().and_then(|v| v.get(0)).cloned()
                        {
                            let upper_threshold = if *min_is_max_temperature {
                                TEMPERATURE_FINE_MAX_OVER_MIN
                            } else {
                                TEMPERATURE_COARSE_MAX_OVER_MIN
                            };

                            if current_temp >= *min_temperature - TEMPERATURE_MIN_UNDER_MIN
                                && current_temp < *min_temperature + upper_threshold
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

                    LineAction::ToolChange { tool } => {
                        // TODO: Make this less of a long running operation.
                        serial_interface.tool_change(*tool).await?;
                        done = true;
                    }

                    LineAction::Pause => {
                        // TOOD: Need to handle keep alive.

                        // TODO: Move to parked position.

                        // TODO: Show a special message and flag the reason that we paused.

                        Self::pause_impl(&shared).await?;

                        done = true;
                    }
                }

                if done {
                    current_action = None;
                    first_stable_time = None;
                    status_message = None;

                    continue;
                } else {
                    if shared.must_keep_alive {
                        // We must continously do something indicating that the
                        // we are still printing from the serial port.
                        // Valid commands are:
                        // https://github.com/prusa3d/Prusa-Firmware-Buddy/blob/master/src/common/serial_printing.cpp#L56
                        //
                        // After the first command, only motion/planner commands will keep the print
                        // alive hence why G4 is used.
                        //
                        // If we don't after 5 seconds, the printer will reset
                        // and clear some state (e.g. on the Prusa XL, the bed
                        // heating area will get reset and the whole bed will
                        // start getting heated).

                        // TODO: Measure the time it takes for this to ensure we haven't gotten
                        // close to the timeout.

                        // TODO: Implement this change for the Prusa XL so the bed doesn't reset:
                        // https://github.com/prusa3d/Prusa-Firmware-Buddy/commit/a1d4041ed7e93a4de1043e1bed85c5eb5ee27f6e

                        serial_interface
                            .send_command("G4 P100\n", DEFAULT_COMMAND_TIMEOUT)
                            .await?;

                        let now = Instant::now();
                        last_alive = now;
                    }

                    // TODO: This will delay the setting of the status message in the state which we
                    // want to avoid (for the first round of waiting on the action).
                    executor::sleep(Duration::from_millis(200)).await?;
                    continue;
                }
            }

            newly_enqueued_lines.clear();

            // Pipelining as many lines as we can onto the serial queue.
            // NOTE: We assume that lines.recv() is fairly fast.
            //
            // TODO: Don't pipeline together multiple very slow commands.
            while !received_all_lines && enqueued_commands.len() < shared.max_enqueued_commands {
                let mut parsed_line = match lines.recv().await {
                    Ok(Some(v)) => v,
                    // All lines have been processed.
                    Ok(None) => {
                        received_all_lines = true;
                        break;
                    }
                    Err(_) => {
                        return Err(err_msg("Exiting command loop since inputs failed"));
                    }
                };

                let mut skipped = false;
                if parsed_line.object >= 0 {
                    let i = parsed_line.object as u32;
                    if cancelled_objects.contains(&i) {
                        skipped = true;

                        // NOTE: State updates are still applied.
                        parsed_line.command_to_send = None;
                        parsed_line.action = None;
                    }
                }

                if let Some(cmd) = parsed_line.command_to_send.take() {
                    enqueued_commands.push_back(
                        serial_interface
                            .enqueue_command(cmd, DEFAULT_COMMAND_TIMEOUT)
                            .await?,
                    );
                }

                if parsed_line.progress_updated {
                    let now = SystemTime::now();
                    parsed_line.state_update.set_last_progress_update(now);
                }

                current_action = parsed_line.action.take();

                newly_enqueued_lines.push((parsed_line, skipped));

                // Note: We don't pipeline across action boundaries.
                if current_action.is_some() {
                    break;
                }
            }

            lock!(state <= shared.state.lock().await?, {
                let num = state.proto.line_number() + (newly_enqueued_lines.len() as u32);
                state.proto.set_line_number(num);

                for (parsed_line, skipped) in newly_enqueued_lines.drain(..) {
                    state.proto.merge_from(&parsed_line.state_update)?;

                    state
                        .proto
                        .objects_mut()
                        .set_current_object_index(parsed_line.object);

                    if parsed_line.object >= 0 {
                        let i = parsed_line.object as usize;
                        if i >= state.proto.objects().objects_len() {
                            return Err(err_msg("Object index out of bounds"));
                        }

                        let obj = &mut state.proto.objects_mut().objects_mut()[i];
                        if skipped {
                            // This may happen if the object was resumed by the use at the same time
                            // that we were running this skipped line.
                            if !obj.has_cancelled() {
                                obj.cancelled_mut().set_cancel_time(SystemTime::now());
                                obj.cancelled_mut().set_cancel_line(num - 1);
                            }

                            let n = obj.cancelled().skipped_lines();
                            obj.cancelled_mut().set_skipped_lines(n + 1);
                        } else {
                            if obj.has_cancelled() {
                                obj.cancelled_mut().set_cancel_line(num);
                            }

                            let n = obj.completed_lines();
                            obj.set_completed_lines(n + 1);
                        }
                    }
                }

                Ok::<_, Error>(())
            })?;
        }

        // TODO: Move this finalization logic to the outer function so that it runs even
        // if the graph fails.

        // Wait for all the commands we sent to finish.
        serial_interface.wait_for_idle().await?;

        // TODO:
        // Wait for current moves to finish.
        // Turn off all heaters/etc.

        /////

        // If we are here, then we finished executing all the lines.

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
        // .set_estimated_remaining_time(Duration::from_secs_f32(v.to_f32() * 60.0));

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
        Self::pause_impl(&self.shared).await
    }

    async fn pause_impl(shared: &Shared) -> Result<()> {
        lock!(state <= shared.state.lock().await?, {
            if state.proto.status() != ProgramRun_PlayerState::PLAYING {
                return Err(
                    rpc::Status::failed_precondition("Player not currently playing.").into(),
                );
            }

            state.proto.set_status(ProgramRun_PlayerState::PAUSING);
            state.notify_all();

            Ok::<_, Error>(())
        })?;

        Self::publish_change(&shared);

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
