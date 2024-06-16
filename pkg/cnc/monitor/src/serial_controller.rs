use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};
use std::{collections::VecDeque, sync::Arc};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::bytes::Bytes;
use common::failure::Fail;
use common::io::{Readable, Writeable};
use executor::channel::oneshot;
use executor::lock;
use executor::sync::{AsyncMutex, AsyncRwLock, AsyncVariable};
use executor_multitask::{impl_resource_passthrough, ServiceResourceGroup};
use file::LocalPath;
use peripherals::serial::SerialPort;

use crate::change::{ChangeEvent, ChangePublisher};
use crate::config::MachineConfigContainer;
use crate::response_parser::*;
use crate::serial_receiver_buffer::SerialReceiverBuffer;
use crate::serial_send_buffer::SerialSendBuffer;

/// Maximum number of commands which can be locally enqueued which haven't been
/// sent yet. Note that sending a message is blocked on getting an 'ok' for the
/// previous
const MAX_LOCAL_QUEUE_LENGTH: usize = 10;

/// Maximum number of bytes we will attempt to read from the serial device in
/// one kernel read.
const READ_BUFFER_SIZE: usize = 512;

/// If we don't receive a status line with the current position of the machine
/// for this amount of time, we will assume that it is dead.
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(4);

/// NOTE: Must an exact multiple of 1 second for platforms that can auto report
/// positions (Marlin/Prusa).
const STATE_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Maximum amount of time we expect it to take for a machine to become healthy
/// after initially connecting to it. Measured from the time the serial port is
/// opened to the point at which the machine gives us back the first valid
/// response.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum amount of time we will wait for commands issued while the machine is
/// idle and don't trigger any physical actuation should take.
const IDLE_COMMAND_TIMEOUT: Duration = Duration::from_millis(200);

pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/*
Need a few timeouts:
1. timeout to receive an 'ok'
2. timeout on how long we are allowed to stay in the queue (can be very long as it is impacted by prior commands.)
*/

// const COMMAND_ACK_DEADLINE: Duration = Duration::from_secs(60);

/*
TODO: What to estimate the serial port baud rate saturation
- 8N1 means 10 bits are sent for every 8 data bits.

Testing print speed:
- https://www.reddit.com/r/ender3/comments/eguib7/speedier_printing_and_the_importance_of_baud_rate/
- Can set Marlin into DRYRUN mode
    - https://marlinfw.org/docs/gcode/M111.html

*/

/*
Open serial port

- Thread reader

Resetting immediately:
- Initially have DTR high
- Then pull it low

*/

#[derive(Clone, Debug, Fail)]
pub enum SendCommandError {
    ReceivedError(String),
    DeadlineExceeded,
    AbruptCancellation,
}

impl std::fmt::Display for SendCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(self, f)
    }
}

pub struct SerialController {
    resources: ServiceResourceGroup,
    shared: Arc<Shared>,
}

impl_resource_passthrough!(SerialController, resources);

struct Shared {
    machine_id: u64,
    config: Arc<AsyncRwLock<MachineConfigContainer>>,
    state: AsyncMutex<State>,
    change_publisher: ChangePublisher,

    sender_pending_buffer: AsyncVariable<SerialPendingSendQueue>,
    receiver_buffer: SerialReceiverBuffer,

    /// Contains the index of the next non-processed line in the receiver
    /// buffer.
    processed_line_waterline: AsyncVariable<u64>,
}

#[derive(Default)]
struct State {
    /// Initially false and set to true when we get the first complete set of
    /// state information back from the machine.
    connected: bool,

    capabilites: HashMap<String, bool>,
    axes: HashMap<String, AxisData>,
    // May have a child task that is running a program (should have up to one of these).
}

#[derive(Default)]
struct SerialPendingSendQueue {
    /// Lines that haven't yet been sent via the serial connection.
    pending_send: VecDeque<PendingSend>,

    /// Line that was written to serial, but hasn't been acknowledged yet via an
    /// ok/error.
    inflight_send: Option<PendingSend>,
}

struct PendingSend {
    /// This is the data to send including a "\n" terminator.
    line: Bytes,

    /// Channel to send the result of the command. Either 'None' if the command
    /// was successful or an error message otherwise.
    callback: oneshot::Sender<Result<(), SendCommandError>>,

    deadline: Instant,
}

struct AxisData {
    data: Vec<f32>,
    last_update: Option<Instant>,
}

impl SerialController {
    pub async fn create(
        machine_id: u64,
        config: Arc<AsyncRwLock<MachineConfigContainer>>,
        serial_reader: Box<dyn Readable>,
        serial_writer: Box<dyn Writeable>,
        change_publisher: ChangePublisher,
    ) -> Result<Self> {
        let resources = ServiceResourceGroup::new("cnc::Machine");

        let mut state = State::default();

        let config_value = config.read().await?;
        for axis_config in config_value.axes() {
            state.axes.insert(
                axis_config.id().to_string(),
                AxisData {
                    data: vec![],
                    last_update: None,
                },
            );
        }

        let shared = Arc::new(Shared {
            machine_id,
            config: config.clone(),
            state: AsyncMutex::new(state),
            sender_pending_buffer: AsyncVariable::default(),
            receiver_buffer: SerialReceiverBuffer::default(),
            change_publisher,
            processed_line_waterline: AsyncVariable::default(),
        });

        resources
            .spawn_interruptable(
                "cnc::Machine::serial_writer",
                Self::serial_writer_thread(shared.clone(), serial_writer),
            )
            .await;

        resources
            .spawn_interruptable(
                "cnc::Machine::serial_reader",
                Self::serial_reader_thread(shared.clone(), serial_reader),
            )
            .await;

        resources
            .spawn_interruptable(
                "cnc::Machine::state_poller",
                Self::state_polling_thread(shared.clone()),
            )
            .await;

        // TODO: Need a 'last breath' mechanism to trigger an emergency stop assuming
        // the serial writer is still healthy.

        Ok(Self { resources, shared })
    }

    /// Is responsible for ensuring that we consistently receiving a report of
    /// the complete state (positions, temperatures, etc.) from the machine.
    async fn state_polling_thread(shared: Arc<Shared>) -> Result<()> {
        // TODO: Block until the reader is done with its timeout.

        let startup_start_time = Instant::now();
        let mut num_error_responses = 0;
        loop {
            let now = Instant::now();
            if now - startup_start_time > STARTUP_TIMEOUT {
                return Err(err_msg("Taking too long for the machine to connect"));
            }

            let res = Self::send_command_inner(&shared, "G21\n", IDLE_COMMAND_TIMEOUT).await;
            eprintln!("{:?}", res);
            match res {
                Ok(()) => break,
                Err(SendCommandError::DeadlineExceeded) => {}
                Err(SendCommandError::ReceivedError(e)) => {
                    num_error_responses += 1;

                    // We allow up to one error response since the machine may have only seen the
                    // last few bytes in the command line.
                    if num_error_responses > 1 {
                        return Err(SendCommandError::ReceivedError(e).into());
                    }
                }
                Err(e) => return Err(e.into()),
            };

            executor::sleep(Duration::from_secs(1)).await?;
        }

        // Wait for any extra stray 'ok' responses to be received from the machine.
        executor::sleep(Duration::from_secs(1)).await?;

        // TODO: Re-set any skew errors.

        eprintln!("Start up done!");

        Self::send_command_inner(&shared, "M115\n", IDLE_COMMAND_TIMEOUT).await?;

        let supports_autoreport = lock!(state <= shared.state.lock().await?, {
            /*
            TODO: Check for all of AUTOREPORT_TEMP,AUTOREPORT_FANS,AUTOREPORT_POSITION
            */

            state
                .capabilites
                .get("AUTOREPORT_POSITION")
                .cloned()
                .unwrap_or(false)
        });

        eprintln!("Supports autoreport: {}", supports_autoreport);

        /*
        // Enter dry run mode.
        Self::send_command_inner(&shared, "M111 S8\n", IDLE_COMMAND_TIMEOUT).await?;
        */

        if supports_autoreport {
            // Setup reporting of everything (temp/position/fans) every 1 seconds.
            // TODO: Check result.
            Self::send_command_inner(&shared, format!("M155 S1 C7\n"), IDLE_COMMAND_TIMEOUT)
                .await?;
        }

        let polling_start_time = Instant::now();

        loop {
            if !supports_autoreport {
                Self::request_state_report_impl(&shared).await?;
            }

            let now = Instant::now();

            let last_received_complete_state = lock!(state <= shared.state.lock().await?, {
                let mut time = None;

                for axis in state.axes.values() {
                    if let Some(t) = axis.last_update {
                        if time.is_none() || t < time.unwrap() {
                            time = Some(t);
                        }
                    }
                }

                if time.is_some() {
                    if !state.connected {
                        eprintln!("Connected!");

                        shared.change_publisher.publish(ChangeEvent::new(
                            EntityType::MACHINE,
                            Some(shared.machine_id),
                            false,
                        ));
                    }

                    state.connected = true;
                }

                time
            });

            if last_received_complete_state.unwrap_or(polling_start_time) + KEEP_ALIVE_TIMEOUT < now
            {
                return Err(err_msg(
                    "Timed out waiting for state information to be received.",
                ));
            }

            executor::sleep(STATE_POLL_INTERVAL).await?;
        }

        Ok(())
    }

    /// Checks if the controller is fully setup and ready to accept user
    /// commands.
    pub async fn connected(&self) -> Result<bool> {
        let state = self.shared.state.lock().await?.read_exclusive();
        Ok(state.connected)
    }

    pub async fn state_proto(&self, proto: &mut MachineStateProto) -> Result<()> {
        let state = self.shared.state.lock().await?.read_exclusive();
        if !state.connected {
            proto.set_connection_state(MachineStateProto_ConnectionState::CONNECTING);
            return Ok(());
        }

        proto.set_connection_state(MachineStateProto_ConnectionState::CONNECTED);

        for (axis_id, axis) in &state.axes {
            let proto = proto.new_axis_values();
            proto.set_id(axis_id);
            proto.value_mut().extend_from_slice(&axis.data);
            if let Some(t) = axis.last_update {
                // TODO: Bring this back.
                // proto.set_last_reported(t);
            }
        }

        Ok(())
    }

    /// TODO: Make this independent of the SerialController
    ///
    /// CANCEL SAFE
    pub async fn read_serial_log(
        &self,
        response: &mut rpc::ServerStreamResponse<'_, ReadSerialLogResponse>,
    ) -> Result<()> {
        let mut next_line_offset = self.shared.receiver_buffer.first_line_offset().await?;

        loop {
            let mut last_line_offset = {
                let waterline = self
                    .shared
                    .processed_line_waterline
                    .lock()
                    .await?
                    .read_exclusive();
                if *waterline == next_line_offset {
                    waterline.wait().await;
                    continue;
                }

                *waterline
            };

            let mut batch = ReadSerialLogResponse::default();

            while next_line_offset < last_line_offset {
                let mut offset = next_line_offset;
                next_line_offset += 1;
                let line = match self.shared.receiver_buffer.get_line(offset).await {
                    Ok(v) => v,
                    // May have been truncated while we were reading
                    Err(e) => continue,
                };

                let mut proto = batch.new_lines();
                proto.set_value(Self::format_bytes(&line.data));
                proto.set_number(offset);
                proto.set_kind(line.kind);
            }

            response.send(batch).await?;
        }
    }

    fn format_bytes(data: &[u8]) -> String {
        let mut out = String::new();
        for b in data {
            if b.is_ascii_alphanumeric() || b.is_ascii_punctuation() || *b == b' ' {
                out.push(*b as char);
            } else {
                out.push_str(&format!("\\x{:X}", *b))
            }
        }

        out
    }

    pub async fn request_state_update(&self) -> Result<()> {
        self.check_clear_to_send().await?;

        Self::request_state_report_impl(&self.shared).await
    }

    async fn request_state_report_impl(shared: &Shared) -> Result<()> {
        // Get position
        Self::send_command_inner(&shared, "M114\n", DEFAULT_COMMAND_TIMEOUT).await?;
        // Get extruder temperatures
        Self::send_command_inner(&shared, "M105\n", DEFAULT_COMMAND_TIMEOUT).await?;

        // TODO: Only do if Marlin/Prusa firmware
        // M123
        Self::send_command_inner(&shared, "M123\n", DEFAULT_COMMAND_TIMEOUT).await?;

        // TODO: Send 'T\n' to get the current tool index.

        Ok(())
    }

    pub async fn set_temperature(&self, axis_id: &str, target: f32) -> Result<()> {
        let config = self.shared.config.read().await?;
        let axis = config
            .axes()
            .iter()
            .find(|a| a.id() == axis_id)
            .ok_or_else(|| {
                rpc::Status::invalid_argument(format!("No axis with id: {}", axis_id))
            })?;

        if axis.typ() != AxisType::HEATER {
            return Err(
                rpc::Status::invalid_argument(format!("Axis {} is not a heater", axis_id)).into(),
            );
        }

        let command = {
            if axis_id == "B" {
                format!("M140 S{:.2}\n", target)
            } else if axis_id == "T" {
                format!("M104 S{:.2}\n", target)
            } else if let Some(num) = axis_id.strip_prefix("T") {
                return Err(err_msg("Setting other tool temps not supported"));
            } else {
                return Err(err_msg("Unsupported heater id"));
            }
        };

        self.send_command(command, DEFAULT_COMMAND_TIMEOUT).await?;

        Ok(())
    }

    pub async fn home_x(&self) -> Result<()> {
        self.send_command("G28 X\n", DEFAULT_COMMAND_TIMEOUT).await
    }

    pub async fn home_y(&self) -> Result<()> {
        self.send_command("G28 Y\n", DEFAULT_COMMAND_TIMEOUT).await
    }

    pub async fn send_command<D: Into<Bytes>>(&self, line: D, timeout: Duration) -> Result<()> {
        self.check_clear_to_send().await?;

        Self::send_command_inner(&self.shared, line, timeout).await?;
        Ok(())
    }

    async fn check_clear_to_send(&self) -> Result<()> {
        let state = self.shared.state.lock().await?.read_exclusive();
        if !state.connected {
            return Err(err_msg(
                "Commands not allowed before the connection is established.",
            ));
        }

        Ok(())
    }

    /// Blocks until we have recieved an ok/error response for the command.
    ///
    /// - timeouts are measured from the time send_command() is called.
    async fn send_command_inner<D: Into<Bytes>>(
        shared: &Shared,
        line: D,
        timeout: Duration,
    ) -> Result<(), SendCommandError> {
        /*
        TODO: If the background tasks fail, then this may never terminate.

        */

        let (sender, receiver) = oneshot::channel();

        let deadline = Instant::now() + timeout;

        let entry = PendingSend {
            line: line.into(),
            callback: sender,
            deadline,
        };

        let queue_guard = shared
            .sender_pending_buffer
            .lock()
            .await
            .map_err(|_| SendCommandError::AbruptCancellation)?;
        lock!(queue <= queue_guard, {
            queue.pending_send.push_back(entry);
            queue.notify_all();
        });

        let res = receiver
            .recv()
            .await
            .map_err(|_| SendCommandError::AbruptCancellation)?;

        res
    }

    async fn serial_writer_thread(
        shared: Arc<Shared>,
        mut writer: Box<dyn Writeable>,
    ) -> Result<()> {
        // Many platforms using will initially boot into the bootloader for a few
        // seconds to wait for flashing commands.
        //
        // This is especially true for Arduino 'reset_using_dtr' style boards which wait
        // longer in the bootloader on explicit resets.
        executor::sleep(Duration::from_millis(5000)).await?;

        // Few empty lines to ensure that any prior commands are well terminated.
        // The first line is an arbitrary string of bytes that should cause parsing to
        // fail for any prior buffered data.
        writer.write_all(b"<>-<>-\n\n").await?;
        // Wait for any errors for the above pre-amble to be skipped.
        executor::sleep(Duration::from_millis(100)).await?;

        loop {
            let mut queue = shared.sender_pending_buffer.lock().await?.enter();

            Self::cancel_exceeded_deadline(&mut queue);

            // Wait for there to be some data to send.
            // We also periodically retry to cancel commands past their deadline.
            if queue.inflight_send.is_some() || queue.pending_send.is_empty() {
                executor::timeout(Duration::from_millis(100), queue.wait()).await;
                continue;
            }

            let data = {
                let next_to_send = queue.pending_send.pop_front().unwrap();
                let data = next_to_send.line.clone();
                queue.inflight_send = Some(next_to_send);
                data
            };

            queue.exit();

            writer.write_all(&data).await?;
        }

        Ok(())
    }

    fn cancel_exceeded_deadline(queue: &mut SerialPendingSendQueue) {
        let now = Instant::now();

        // TODO: We need to measure in-flight timeout from the time it was sent to avoid
        // forgetting about it too soon and losing sync.
        if let Some(send) = &queue.inflight_send {
            if send.deadline < now {
                queue
                    .inflight_send
                    .take()
                    .unwrap()
                    .callback
                    .send(Err(SendCommandError::DeadlineExceeded));
            }
        }

        while !queue.pending_send.is_empty() {
            let send = &queue.pending_send[0];
            if send.deadline < now {
                queue
                    .pending_send
                    .pop_front()
                    .unwrap()
                    .callback
                    .send(Err(SendCommandError::DeadlineExceeded));
            } else {
                break;
            }
        }
    }

    async fn serial_reader_thread(
        shared: Arc<Shared>,
        mut reader: Box<dyn Readable>,
    ) -> Result<()> {
        // Absolute offset of the next received line that needs to be
        let mut next_line_offset = shared.receiver_buffer.last_line_offset().await?;

        loop {
            let mut buf = [0u8; READ_BUFFER_SIZE];
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                // NOTE: This will be triggered is this is a USB serial device that was
                // disconnected.
                return Err(err_msg("Hit end of the serial read end"));
            }

            let now = Instant::now();

            // TODO: Consider not erroring out if there are extremely long lines.
            shared.receiver_buffer.append(&buf[0..n], now).await?;

            let config = shared.config.read().await?;

            let mut state = shared.state.lock().await?.enter();

            let mut got_state_change = false;

            // Process any newly added lines.
            let end_line_offset = shared.receiver_buffer.last_line_offset().await?;
            while next_line_offset < end_line_offset {
                let line = shared.receiver_buffer.get_line(next_line_offset).await?;
                next_line_offset += 1;

                let mut events = vec![];
                if let Err(e) = parse_response_line(&line.data, &config, &mut events) {
                    eprintln!("Failure parsing response line: {}", e);
                    continue;
                }

                let mut command_response = None;

                // println!("{:?}", events);

                let mut kind = ReadSerialLogResponse_LineKind::UNKNOWN;

                for event in events {
                    match event {
                        ResponseEvent::Ok => {
                            command_response = Some(Ok(()));
                            kind = ReadSerialLogResponse_LineKind::OK;
                        }
                        ResponseEvent::Error { message } => {
                            command_response = Some(Err(SendCommandError::ReceivedError(message)));
                            kind = ReadSerialLogResponse_LineKind::ERROR;
                        }
                        ResponseEvent::Echo { message } => {
                            // TODO: Do something!
                        }
                        ResponseEvent::Capability { name, present } => {
                            state.capabilites.insert(name, present);
                            got_state_change = true;
                        }
                        ResponseEvent::AxisValue { id, values } => {
                            state.axes.insert(
                                id,
                                AxisData {
                                    data: values,
                                    last_update: Some(line.time),
                                },
                            );
                            got_state_change = true;

                            // TODO: Sometimes state updates are on 'ok' lines so could be
                            // classified as either this or 'ok' kind.
                            kind = ReadSerialLogResponse_LineKind::STATE_UPDATE;
                        }
                    }
                }

                shared
                    .receiver_buffer
                    .set_kind(next_line_offset - 1, kind)
                    .await?;

                // NOTE: We only respond to commands after the entire line is processed since
                // there is often response data for the command on the same line as the 'ok'.
                if let Some(res) = command_response {
                    lock!(queue <= shared.sender_pending_buffer.lock().await?, {
                        if let Some(entry) = queue.inflight_send.take() {
                            entry.callback.send(res);
                            queue.notify_all();
                        } else {
                            // TODO: Make this a error after the connection is established.
                            eprintln!("Received response without a command! {:?}", res);
                        }
                    });
                }

                // TODO: Maybe send an event here.

                // TODO: Delete or move logic to me.
                // Self::process_line(&line);
            }

            // TOOD: Improve this.
            state.exit();

            drop(config);

            lock!(
                waterline <= shared.processed_line_waterline.lock().await?,
                {
                    if next_line_offset != *waterline {
                        *waterline = next_line_offset;
                        waterline.notify_all();
                    }
                }
            );

            if got_state_change {
                shared.change_publisher.publish(ChangeEvent::new(
                    EntityType::MACHINE,
                    Some(shared.machine_id),
                    false,
                ));
            }

            // Having non-ascii responses should probably trigger a warning.
        }
    }
}
