use std::collections::{HashMap, HashSet};
use std::string::String;
use std::time::{Duration, Instant, SystemTime};
use std::{collections::VecDeque, sync::Arc};

use base_error::*;
use base_util::format::format_bytes;
use cnc_monitor_proto::cnc::*;
use common::bytes::Bytes;
use common::failure::Fail;
use common::fixed::vec::FixedVec;
use common::hash::FastHasherBuilder;
use common::io::{Readable, Writeable};
use executor::channel::oneshot;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::{AsyncMutex, AsyncRwLock, AsyncVariable, SyncMutex};
use executor_multitask::{impl_resource_passthrough, ServiceResourceGroup};
use file::LocalPath;
use math::matrix::Dimension;
use net::ip::SocketAddr;
use net::udp::UdpBindOptions;
use peripherals::serial::SerialPort;

use crate::change::{ChangeEvent, ChangePublisher};
use crate::config::MachineConfigContainer;
use crate::metric::{MetricStore, MetricStream};
use crate::response_parser::*;
use crate::serial_receiver_buffer::SerialReceiverBuffer;
use crate::serial_send_buffer::SerialSendBuffer;
use crate::syslog_parser::{parse_prusa_metrics_packet, InfluxDBValue};
use crate::timestamped_value::*;
use crate::connection_controller::*;

/// Maximum number of commands which can be locally enqueued which haven't been
/// sent yet. Note that sending a message is blocked on getting an 'ok' for the
/// previous
///
/// TODO: Need to expose this in the config as well
const MAX_LOCAL_QUEUE_LENGTH: usize = 10;

/// Maximum number of bytes we will attempt to read from the serial device in
/// one kernel read.
const READ_BUFFER_SIZE: usize = 1024;

/// If we don't receive a status line with the current position of the machine
/// for this amount of time, we will assume that it is dead.
///
/// TODO: Make this much lower (20 seconds) for non-Carvera machines.
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Max age of state data which we don't reliably get at a high frequently. In
/// gRBL this is stuff that requires calling '$G' and '$#' since these block on
/// the prior
const LOW_FREQUENCY_KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// NOTE: Must an exact multiple of 1 second for platforms that can auto report
/// positions (Marlin/Prusa).
///
/// This needs to be fairly slow for fast machines like 3d printers with MCU
/// based gcode processing since too much polling can messa up motions.
const STATE_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Maximum amount of time we expect it to take for a machine to become healthy
/// after initially connecting to it. Measured from the time the serial port is
/// opened to the point at which the machine gives us back the first valid
/// response.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum amount of time we will wait for commands issued while the machine is
/// idle and don't trigger any physical actuation should take.
pub const IDLE_COMMAND_TIMEOUT: Duration = Duration::from_millis(200);

/// This mainly needs to be fairly long since commands like tool changes and
/// mesh leveling can take a while.
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(10 * 60);

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

    // TODO: Share an instance of this across instances (though will need to clear the state
    // between runs).
    receiver_buffer: SerialReceiverBuffer,

    /// Contains the index of the next non-processed line in the receiver
    /// buffer.
    processed_line_waterline: AsyncVariable<u64>,

    axis_metrics: HashMap<String, Vec<MetricStream>, FastHasherBuilder>,

    syslog_handler_addr: Option<SocketAddr>,

    has_fan_axes: bool,
}

/// TODO: Move this into a separate file since we need all the TimestampedValue
/// usages to be synced across the serialization/merging/checking code.
#[derive(Default)]
struct State {
    /// Initially false and set to true when we get the first complete set of
    /// state information back from the machine.
    connected: bool,

    capabilities: HashMap<String, bool, FastHasherBuilder>,

    axes: HashMap<String, AxisData, FastHasherBuilder>,

    /// TODO: Must track the last update time of this info.
    ///
    /// Will be None if the machine firmware doesn't report a string state.
    firmware_state: TimestampedValue<String>,

    spindle: SpindleData,

    /// A value of 0 is used when coordinate systems are not supported (to
    /// indicate that we are in the machine coordinate system)
    ///
    /// TODO: Must track the last update time of this data.
    current_coordinate_system: TimestampedValue<u32>,

    /// Note that the indexes start at 1 (for G54).
    coordinate_systems: HashMap<u32, CoordinateSystemData, FastHasherBuilder>,

    /// This will be None if the machine doesn't have an auto tool changer.
    active_tool: TimestampedValue<i32>,

    disabled_syslog_metrics: HashSet<String, FastHasherBuilder>,
}

#[derive(Default)]
struct SpindleData {
    current_rpm: TimestampedValue<f32>,
    target_rpm: TimestampedValue<f32>,
    mode: TimestampedValue<SpindleState_Mode>,
}

#[derive(Default, Debug)]
struct AutoReportSupport {
    temps: bool,
    position: bool,
    fans: bool,
}

impl AutoReportSupport {
    fn some_not_supported(&self) -> bool {
        !self.temps || !self.position || !self.fans
    }
}

define_bit_flags!(
    SendCommandFlags u32 {
        /// If true, the command is pushed to the front of the queue and will be
        /// sent at the next moment possible.
        SKIP_LINE = (1 << 0),

        /// This command will trigger a full stop of the machine so we should stop sending any further commands after this one.
        STOP_AFTER = (1 << 1),

        /// This command will not get any acknowledgement 'ok'/'error' line.
        /// NOTE: Currently
        NO_REPLY = (1 << 2)
    }
);

#[derive(Default)]
struct SerialPendingSendQueue {
    /// When true, the sending thread has stopped so can't send any more
    /// commands.
    stopped: bool,

    /// Lines that haven't yet been sent via the serial connection.
    pending_send: VecDeque<PendingSend>,

    /// Lines that was written to serial, but hasn't been acknowledged yet via
    /// an ok/error.
    inflight_send: VecDeque<PendingSend>,
}

struct PendingSend {
    /// This is the data to send including a "\n" terminator if it will get a
    /// reply.
    line: Bytes,

    /// Channel to send the result of the command. Either 'None' if the command
    /// was successful or an error message otherwise.
    callback: oneshot::Sender<Result<(), SendCommandError>>,

    deadline: Instant,

    no_reply: bool,
}

#[derive(Default)]
pub struct CoordinateSystemData {
    /// CoordinateSystemPosition = MachinePosition - Offset
    offset: HashMap<String, AxisData, FastHasherBuilder>,
}

/// This ensures that if the threads die, we will cancel all outstanding send
/// requests. Without this, send_command futures started shortly before the
/// connection fails may never terminate.
struct SenderCancellationGuard {
    shared: Arc<Shared>,
}

impl Drop for SenderCancellationGuard {
    fn drop(&mut self) {
        let shared = self.shared.clone();
        executor::spawn(async move {
            let state = match shared.sender_pending_buffer.lock().await {
                Ok(v) => v,
                Err(e) => return,
            };

            lock!(state <= state, {
                state.stopped = true;

                state.pending_send.clear();
                state.inflight_send.clear();
            });
        });
    }
}

struct ReceiverClosedGuard {
    shared: Arc<Shared>,
}

impl Drop for ReceiverClosedGuard {
    fn drop(&mut self) {
        let shared = self.shared.clone();
        executor::spawn(async move {
            let state = match shared.state.lock().await {
                Ok(v) => v,
                Err(_) => return,
            };

            lock!(state <= state, {
                state.connected = false;
            });
        });
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum RateLimitedEvent {
    StateUpdate,
    DiagnosticString,
    ParserState,
}

#[derive(Default)]
struct RateLimiterState {
    last_event_time: HashMap<RateLimitedEvent, Instant, FastHasherBuilder>,
}

impl RateLimiterState {
    fn should_allow(&mut self, event: RateLimitedEvent) -> bool {
        let now = Instant::now();

        let rate = match event {
            RateLimitedEvent::StateUpdate => Duration::from_millis(200), // 5Hz
            RateLimitedEvent::DiagnosticString => Duration::from_millis(1000),
            // TODO: THis is currently unused
            RateLimitedEvent::ParserState => Duration::from_millis(2000),
        };

        match self.last_event_time.get(&event) {
            Some(last_time) => {
                if now.duration_since(*last_time) < rate {
                    return false;
                }
            }
            None => {}
        }

        self.last_event_time.insert(event, now);

        true
    }
}

impl SerialController {
    pub async fn create(
        machine_id: u64,
        config: Arc<AsyncRwLock<MachineConfigContainer>>,
        serial_reader: Box<dyn Readable>,
        serial_writer: Box<dyn Writeable>,
        change_publisher: ChangePublisher,
        metric_store: &MetricStore,
    ) -> Result<Self> {
        let resources = ServiceResourceGroup::new("cnc::Machine");

        let mut state = State::default();

        let mut axis_metrics = HashMap::default();

        let config_value = config.read().await?;
        for axis_config in config_value.axes() {
            state.axes.insert(
                axis_config.id().to_string(),
                AxisData {
                    data: TimestampedValue::default(),
                },
            );

            let mut num_values = {
                // TODO: THis should also be 2 for switches if they are controllable.
                if axis_config.typ() == AxisType::HEATER {
                    2
                } else {
                    1
                }
            };

            if axis_config.has_collect() {
                let mut streams = vec![];
                for i in 0..num_values {
                    let mut resource = MetricResource::default();
                    resource.set_machine_id(machine_id);
                    resource.set_kind(MetricKind::MACHINE_AXIS_VALUE);
                    resource.set_axis_id(axis_config.id());
                    resource.set_value_index(i as u32);

                    let stream = metric_store.stream(&resource).await?;
                    streams.push(stream);
                }

                axis_metrics.insert(axis_config.id().to_string(), streams);
            }
        }

        if Self::supports_coordinate_systems(&config_value) {
            for (i, code) in gcode::STANDARD_COORDINATE_SYSTEMS.iter().enumerate() {
                state
                    .coordinate_systems
                    .insert((i + 1) as u32, CoordinateSystemData::default());
            }
        }

        let mut syslog_handler_addr = None;
        if config_value.has_syslog_handler() {
            let addr = config_value.syslog_handler().addr().parse()?;
            syslog_handler_addr = Some(addr);
        }

        let mut has_fan_axes = false;
        for axis in config_value.axes() {
            if axis.typ() == AxisType::FAN_PWM_VALUE || axis.typ() == AxisType::FAN_TACHOMETER_RPM {
                has_fan_axes = true;
                break;
            }
        }

        let shared = Arc::new(Shared {
            machine_id,
            config: config.clone(),
            state: AsyncMutex::new(state),
            sender_pending_buffer: AsyncVariable::default(),
            receiver_buffer: SerialReceiverBuffer::default(),
            change_publisher,
            processed_line_waterline: AsyncVariable::default(),
            axis_metrics,
            syslog_handler_addr,
            has_fan_axes,
        });

        let sender_guard = SenderCancellationGuard {
            shared: shared.clone(),
        };

        let receiver_guard = ReceiverClosedGuard {
            shared: shared.clone(),
        };

        resources
            .spawn_interruptable(
                "cnc::Machine::serial_writer",
                Self::serial_writer_thread(shared.clone(), serial_writer, sender_guard),
            )
            .await;

        resources
            .spawn_interruptable(
                "cnc::Machine::serial_reader",
                Self::serial_reader_thread(shared.clone(), serial_reader, receiver_guard),
            )
            .await;

        resources
            .spawn_interruptable(
                "cnc::Machine::state_poller",
                Self::state_polling_thread(shared.clone()),
            )
            .await;

        if shared.syslog_handler_addr.is_some() {
            resources
                .spawn_interruptable(
                    "cnc::Machine::syslog_handler_thread",
                    Self::syslog_handler_thread(shared.clone()),
                )
                .await;

            resources
                .spawn_interruptable(
                    "cnc::Machine::syslog_metric_disabler_thread",
                    Self::syslog_metric_disabler_thread(shared.clone()),
                )
                .await;
        }

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

            // Send a 'Set Units to Millimeters' command. Every firmware should support
            // this.
            let res = Self::send_command_inner(
                &shared,
                "G21\n".into(),
                IDLE_COMMAND_TIMEOUT,
                SendCommandFlags::empty(),
            )
            .await;

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

        let firmware = shared.config.read().await?.firmware();

        match firmware {
            MachineConfig_Firmware::GENERIC => {
                // There are no generic state polling capabilities so immediately assume we are
                // connected.

                lock!(state <= shared.state.lock().await?, {
                    shared.change_publisher.publish(ChangeEvent::new(
                        EntityType::MACHINE,
                        Some(shared.machine_id),
                        false,
                    ));

                    state.connected = true;
                });
            }
            MachineConfig_Firmware::MARLIN | MachineConfig_Firmware::PRUSA => {
                Self::state_polling_marlin(&shared).await?;
            }
            MachineConfig_Firmware::SMOOTHIEWARE
            | MachineConfig_Firmware::GRBL
            | MachineConfig_Firmware::CARVERA => {
                Self::state_polling_grbl(&shared).await?;
            }
            _ => return Err(format_err!("Unsupported firmware: {:?}", firmware)),
        }

        Ok(())
    }

    async fn state_polling_marlin(shared: &Shared) -> Result<()> {
        // Poll capabilities.
        Self::send_command_inner(
            &shared,
            "M115\n".into(),
            IDLE_COMMAND_TIMEOUT,
            SendCommandFlags::empty(),
        )
        .await?;

        let mut autoreport_support = lock!(state <= shared.state.lock().await?, {
            AutoReportSupport {
                position: state
                    .capabilities
                    .get("AUTOREPORT_POSITION")
                    .cloned()
                    .unwrap_or(false),
                temps: state
                    .capabilities
                    .get("AUTOREPORT_TEMP")
                    .cloned()
                    .unwrap_or(false),
                fans: state
                    .capabilities
                    .get("AUTOREPORT_FANS")
                    .cloned()
                    .unwrap_or(false),
            }
        });

        // In old Marlin and latest Prusa firmware versions, M114 will cause stuttering:
        // https://github.com/prusa3d/Prusa-Firmware-Buddy/issues/2788
        //
        // Until then we can't easily poll for position data. Instead we must rely on
        // syslog position information.
        {
            let config = shared.config.read().await?;
            if config.firmware() == MachineConfig_Firmware::PRUSA {
                autoreport_support.position = true;
            }
        }

        /*
        // Enter dry run mode.
        Self::send_command_inner(&shared, "M111 S8\n", IDLE_COMMAND_TIMEOUT).await?;
        */

        // TODO: Configure 'silent_mode' (either enable or disable if supported).

        if autoreport_support.fans || autoreport_support.position || autoreport_support.temps {
            // Setup reporting of everything (temp/position/fans) every 1 seconds.
            // TODO: Check result.
            Self::send_command_inner(
                &shared,
                format!("M155 S1 C7\n").into(),
                IDLE_COMMAND_TIMEOUT,
                SendCommandFlags::empty(),
            )
            .await?;
        }

        // Enable syslog handler.
        if let Some(addr) = &shared.syslog_handler_addr {
            let cmd = format!("M334 {} {}\n", addr.ip().to_string(), addr.port());

            Self::send_command_inner(
                &shared,
                cmd.into(),
                IDLE_COMMAND_TIMEOUT,
                SendCommandFlags::empty(),
            )
            .await?;

            for metric in ["pos_x", "pos_y", "pos_z"] {
                Self::send_command_inner(
                    &shared,
                    format!("M331 {}\n", metric).into(),
                    IDLE_COMMAND_TIMEOUT,
                    SendCommandFlags::empty(),
                )
                .await?;
            }
        }

        let polling_start_time = Instant::now();

        loop {
            Self::request_state_report_marlin(shared, &autoreport_support).await?;

            Self::check_machine_connected(
                shared,
                polling_start_time,
                autoreport_support.some_not_supported(),
            )
            .await?;

            executor::sleep(STATE_POLL_INTERVAL).await?;
        }

        Ok(())
    }

    async fn state_polling_grbl(shared: &Arc<Shared>) -> Result<()> {
        // Normalize the initial state of the machine
        // - Workspace 1 coordinate system
        // ...
        for line in ["G54 G17 G21\n", "G90 G92.1 M5\n"] {
            Self::send_command_inner(
                &shared,
                line.into(),
                IDLE_COMMAND_TIMEOUT,
                SendCommandFlags::empty(),
            )
            .await?;
        }

        let polling_start_time = Instant::now();

        let mut rate_limiter = RateLimiterState::default();

        let mut update_done = Arc::new(SyncMutex::new(true));

        let shared2 = shared.clone();
        let thread = ChildTask::spawn(async move {
            loop {
                // TODO: Check this result.
                let _ = Self::request_state_report_grbl_slow(&shared2).await;

                executor::sleep(Duration::from_millis(5000)).await;
            }
        });

        loop {
            Self::request_state_report_grbl(shared, &mut rate_limiter).await?;

            // TODO: Need some support for optional axes as things may not exist when
            // TODO: Throttle this to 1hz
            Self::check_machine_connected(shared, polling_start_time, false).await?;

            executor::sleep(Duration::from_millis(1000)).await?;
        }

        Ok(())
    }

    async fn check_machine_connected(
        shared: &Shared,
        polling_start_time: Instant,
        command_based_polling: bool,
    ) -> Result<()> {
        let config = shared.config.read().await?;

        let now = Instant::now();

        let mut normal_timeout = KEEP_ALIVE_TIMEOUT;
        let mut low_freq_timeout = LOW_FREQUENCY_KEEP_ALIVE_TIMEOUT;

        // If we need to wait in the command queue for polling state updates (Prusa
        // 32-bit machines that don't autoreport position/fans), we must wait the max
        // time a command could take.
        if command_based_polling {
            normal_timeout = DEFAULT_COMMAND_TIMEOUT;
            low_freq_timeout = DEFAULT_COMMAND_TIMEOUT;
        }

        struct TimeTracker {
            normal_timeout: Duration,
            missing_values: Vec<String>,
            now: Instant,
        }

        impl TimeTracker {
            fn check<F: FnOnce() -> String>(&mut self, t: Option<Instant>, f: F) {
                self.check_with_ttl(t, f, self.normal_timeout)
            }

            fn check_with_ttl<F: FnOnce() -> String>(
                &mut self,
                t: Option<Instant>,
                f: F,
                ttl: Duration,
            ) {
                if let Some(t) = t {
                    if t >= self.now - ttl {
                        return;
                    }
                }

                self.missing_values.push(f());
            }
        }

        /// List of value name without a recent value.
        let mut tracker = TimeTracker {
            normal_timeout,
            missing_values: vec![],
            now,
        };

        let check_time = |t: Option<Instant>, f: fn() -> String| {};

        lock!(state <= shared.state.lock().await?, {
            for (axis_id, axis) in &state.axes {
                let axis_config = match config.axes_map().get(axis_id) {
                    Some(v) => v,
                    None => panic!(),
                };

                if axis_config.is_optional() {
                    continue;
                }

                tracker.check(axis.data.last_updated(), || axis_id.clone());
            }

            // TODO: Only if supported.
            if Self::supports_firmware_state(&config) {
                tracker.check(state.firmware_state.last_updated(), || {
                    "firmware_state".to_string()
                });
            }

            if config.has_spindle() {
                if config.spindle().supports_current_speed() {
                    tracker.check(state.spindle.current_rpm.last_updated(), || {
                        "spindle.current_rpm".into()
                    });
                }
                tracker.check(state.spindle.mode.last_updated(), || "spindle.mode".into());
                tracker.check(state.spindle.target_rpm.last_updated(), || {
                    "spindle.target_rpm".into()
                });
            }

            if Self::supports_coordinate_systems(&config) {
                tracker.check_with_ttl(
                    state.current_coordinate_system.last_updated(),
                    || "current_coordinate_system".into(),
                    low_freq_timeout,
                );
            }

            // coordinate_systems
            for (idx, coordinate_system) in &state.coordinate_systems {
                // We don't configure ahead of time which axes will be offset in the coordinate
                // systems, but we assume that once we get the offsets once, we get them for all
                // axes at the same time.
                if coordinate_system.offset.is_empty() {
                    tracker.missing_values.push(format!("Offset{}", idx));
                    continue;
                }

                for (axis, data) in &coordinate_system.offset {
                    tracker.check_with_ttl(
                        data.data.last_updated(),
                        || format!("Offset{}:{}", idx, axis),
                        low_freq_timeout,
                    );
                }
            }

            if Self::supports_tool_changer(&config) {
                tracker.check(state.active_tool.last_updated(), || "active_tool".into());
            }

            if !tracker.missing_values.is_empty() {
                return;
            }

            if !state.connected {
                shared.change_publisher.publish(ChangeEvent::new(
                    EntityType::MACHINE,
                    Some(shared.machine_id),
                    false,
                ));
            }

            state.connected = true;
        });

        if !tracker.missing_values.is_empty() && now > polling_start_time + KEEP_ALIVE_TIMEOUT {
            return Err(format_err!(
                "Timed out waiting for state information to be received; Missing data for: {}",
                tracker.missing_values.join(", ")
            ));
        }

        Ok(())
    }

    async fn syslog_handler_thread(shared: Arc<Shared>) -> Result<()> {
        let addr = shared.syslog_handler_addr.clone().unwrap();

        let socket = net::udp::UdpSocket::bind_with_options(
            addr,
            UdpBindOptions::new().reuse_addr(true).reuse_port(true),
        )
        .await?;

        let mut buf = vec![0u8; 1024];

        loop {
            let (n, addr) = socket.recv_from(&mut buf[..]).await?;
            let data = &buf[0..n];

            let config = shared.config.read().await?;

            let mut new_state_data = State::default();
            let mut time = Instant::now();
            let mut has_state_change = false;

            let points = match parse_prusa_metrics_packet(data) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!(
                        "[{}] Failed to parse syslog packet: {}: {}",
                        addr.to_string(),
                        e,
                        format_bytes(data)
                    );
                    continue;
                }
            };

            for point in points {
                if point.measurement == "active_extruder" {
                    // TODO: Allow extra fields to be added in the future.
                    if point.fields.len() != 1 || point.fields[0].0 != "v" {
                        return Err(err_msg("Unknown fields in active_extruder metric"));
                    }

                    let mut v = match point.fields[0].1 {
                        InfluxDBValue::Integer(v) => v,
                        _ => {
                            return Err(format_err!(
                                "Unexpected format for active_extruder field value: {:?}",
                                point.fields[0].1
                            ));
                        }
                    } as i32;

                    if v >= (config.tools().num_slots() as i32) {
                        v = -1;
                    }

                    new_state_data.active_tool = TimestampedValue::new(v, time);
                    has_state_change = true;
                    continue;
                }

                if let Some(axis) = point.measurement.strip_prefix("pos_") {
                    if point.fields.len() != 1 || point.fields[0].0 != "v" {
                        return Err(err_msg("Unknown fields in active_extruder metric"));
                    }

                    let axis = axis.to_ascii_uppercase();
                    if axis != "X" && axis != "Y" && axis != "Z" {
                        return Err(err_msg("Unsupported position axis"));
                    }

                    let mut v = match point.fields[0].1 {
                        InfluxDBValue::Float(v) => v,
                        _ => {
                            return Err(format_err!(
                                "Unexpected format for active_extruder field value: {:?}",
                                point.fields[0].1
                            ));
                        }
                    };

                    let mut values = FixedVec::new();
                    values.push(v);

                    new_state_data.axes.insert(
                        axis,
                        AxisData {
                            data: TimestampedValue::new(values, time),
                        },
                    );
                    has_state_change = true;
                    continue;
                }

                // Any metrics we don't use will just create extra CPU load on the MCU to send
                // back so prefer to disable them.
                new_state_data
                    .disabled_syslog_metrics
                    .insert(point.measurement.clone());
                has_state_change = true;
            }

            // TODO: Deduplicate the merging of the states.
            if has_state_change {
                lock!(state <= shared.state.lock().await?, {
                    state.axes.extend(new_state_data.axes.drain());

                    state
                        .active_tool
                        .insert_if_present(new_state_data.active_tool.take());

                    state
                        .disabled_syslog_metrics
                        .extend(new_state_data.disabled_syslog_metrics.drain());
                });
            }
        }

        Ok(())
    }

    async fn syslog_metric_disabler_thread(shared: Arc<Shared>) -> Result<()> {
        loop {
            // M332 metric_to_disable

            let metrics_to_disable = lock!(state <= shared.state.lock().await?, {
                state.disabled_syslog_metrics.clone()
            });

            for metric in &metrics_to_disable {
                Self::send_command_inner(
                    &shared,
                    format!("M332 {}\n", metric).into(),
                    DEFAULT_COMMAND_TIMEOUT,
                    SendCommandFlags::empty(),
                )
                .await?;
                executor::sleep(Duration::from_millis(100)).await?;
            }

            if !metrics_to_disable.is_empty() {
                lock!(state <= shared.state.lock().await?, {
                    state.disabled_syslog_metrics.clear();
                })
            }

            executor::sleep(Duration::from_secs(2)).await?;
        }
    }

    async fn request_state_report_marlin(
        shared: &Shared,
        autoreport_support: &AutoReportSupport,
    ) -> Result<()> {
        // Get position.
        if !autoreport_support.position && !shared.syslog_handler_addr.is_some() {
            Self::send_command_inner(
                &shared,
                "M114\n".into(),
                DEFAULT_COMMAND_TIMEOUT,
                SendCommandFlags::SKIP_LINE,
            )
            .await?;
        }

        // Get extruder temperatures
        if !autoreport_support.temps {
            Self::send_command_inner(
                &shared,
                "M105\n".into(),
                DEFAULT_COMMAND_TIMEOUT,
                SendCommandFlags::SKIP_LINE,
            )
            .await?;
        }

        // TODO: Only do if Marlin/Prusa firmware
        // M123
        if shared.has_fan_axes && !autoreport_support.fans {
            Self::send_command_inner(
                &shared,
                "M123\n".into(),
                DEFAULT_COMMAND_TIMEOUT,
                SendCommandFlags::SKIP_LINE,
            )
            .await?;
        }

        Ok(())
    }

    // TODO: Allow users to call both this and the other one.
    async fn request_state_report_grbl(
        shared: &Shared,
        rate_limits: &mut RateLimiterState,
    ) -> Result<()> {
        // Current position information
        if rate_limits.should_allow(RateLimitedEvent::StateUpdate) {
            Self::send_command_inner(
                &shared,
                "?".into(),
                DEFAULT_COMMAND_TIMEOUT,
                SendCommandFlags::SKIP_LINE | SendCommandFlags::NO_REPLY,
            )
            .await?;

            // The response payload is fairly large, so avoid congesting the line.
            executor::sleep(Duration::from_millis(10)).await?;
        }

        // Carvera specific diagnostic info.
        if rate_limits.should_allow(RateLimitedEvent::DiagnosticString) {
            Self::send_command_inner(
                &shared,
                "*".into(),
                DEFAULT_COMMAND_TIMEOUT,
                SendCommandFlags::SKIP_LINE | SendCommandFlags::NO_REPLY,
            )
            .await?;

            // The response payload is fairly large, so avoid congesting the line.
            executor::sleep(Duration::from_millis(10)).await?;
        }

        Ok(())
    }

    async fn request_state_report_grbl_slow(shared: &Shared) -> Result<()> {
        // TODO: Try to remove the dependency on these since they will block the
        // microcontroller while printing out the very long responses.

        let _ = Self::send_command_inner(
            &shared,
            "$G\n".into(),
            DEFAULT_COMMAND_TIMEOUT,
            SendCommandFlags::SKIP_LINE,
        )
        .await;

        executor::sleep(Duration::from_millis(500)).await?;

        let _ = Self::send_command_inner(
            &shared,
            "$#\n".into(),
            DEFAULT_COMMAND_TIMEOUT,
            SendCommandFlags::SKIP_LINE,
        )
        .await;

        Ok(())
    }

    /// All the gRBL derived firmwares.
    fn supports_coordinate_systems(config: &MachineConfig) -> bool {
        config.firmware() == MachineConfig_Firmware::GRBL
            || config.firmware() == MachineConfig_Firmware::SMOOTHIEWARE
            || config.firmware() == MachineConfig_Firmware::CARVERA
    }

    /// All the gRBL derived firmwares.
    fn supports_firmware_state(config: &MachineConfig) -> bool {
        Self::supports_coordinate_systems(config)
    }

    fn supports_tool_changer(config: &MachineConfig) -> bool {
        config.tools().num_slots() > 0
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
    async fn send_command_inner(
        shared: &Shared,
        line: Bytes,
        timeout: Duration,
        flags: SendCommandFlags,
    ) -> Result<(), SendCommandError> {
        Self::enqueue_command_inner(shared, line, timeout, flags)
            .await?
            .wait()
            .await
    }

    async fn enqueue_command_inner(
        shared: &Shared,
        line: Bytes,
        timeout: Duration,
        flags: SendCommandFlags,
    ) -> Result<PendingCommand, SendCommandError> {
        // TODO: Should have the gcode size limiter somewhere here

        let (sender, receiver) = oneshot::channel();

        let deadline = Instant::now() + timeout;

        let entry = PendingSend {
            line,
            callback: sender,
            deadline,
            no_reply: flags.contains(SendCommandFlags::NO_REPLY),
        };

        let queue_guard = shared
            .sender_pending_buffer
            .lock()
            .await
            .map_err(|_| SendCommandError::AbruptCancellation)?;
        lock!(queue <= queue_guard, {
            if queue.stopped {
                return Err(SendCommandError::AbruptCancellation);
            }

            if flags.contains(SendCommandFlags::STOP_AFTER) {
                queue.stopped = true;
                if flags.contains(SendCommandFlags::SKIP_LINE) {
                    queue.pending_send.clear();
                }
            }

            if flags.contains(SendCommandFlags::SKIP_LINE) {
                queue.pending_send.push_front(entry);
            } else {
                queue.pending_send.push_back(entry);
            }

            queue.notify_all();

            Ok::<_, SendCommandError>(())
        })?;

        Ok(PendingCommand::new(receiver))
    }


    async fn serial_writer_thread(
        shared: Arc<Shared>,
        mut writer: Box<dyn Writeable>,
        sender_guard: SenderCancellationGuard,
    ) -> Result<()> {
        let add_quiet_period;
        let max_in_flight;
        {
            let config = shared.config.read().await?;

            add_quiet_period = config.firmware() == MachineConfig_Firmware::SMOOTHIEWARE
                || config.firmware() == MachineConfig_Firmware::CARVERA;
            max_in_flight = core::cmp::max(config.serial().max_in_flight_commands(), 1) as usize;
        }

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

        let mut just_sent_no_reply_command = false;

        loop {
            /*
            // Some firmwares (at least confirmed on Carvera firmware) seem to be
            // susceptible to memory corruption if we send UART commands too fast. So this
            // mitigates this issue by ensuring that there is a minimum quiet period between
            // command lines to ensure that previous lines are mostly done processing before
            // new ones are processed.
            //
            // NOTE: This isn't a perfect fix and there is still a small risk of corruption
            // even with this wait.
            if add_quiet_period {
                if just_sent_no_reply_command {
                    executor::sleep(Duration::from_millis(100)).await?;
                    just_sent_no_reply_command = false;
                }
                // else {
                //     executor::sleep(Duration::from_millis(20)).await?;
                // }
            }
            */
            
            let mut queue = shared.sender_pending_buffer.lock().await?.enter();

            Self::cancel_exceeded_deadline(&mut queue);

            // Special case for 'no-reply' commands. Always immediately send them under the
            // assumption that they are small enough to not cause any issues.
            if let Some(send) = queue.pending_send.front() {
                if send.no_reply {
                    let send = queue.pending_send.pop_front().unwrap();
                    queue.exit();
                    writer.write_all(&send.line).await?;
                    send.callback.send(Ok(()));
                    just_sent_no_reply_command = true;
                    continue;
                }
            }

            // Wait for there to be some data to send.
            // We also periodically retry to cancel commands past their deadline.
            if queue.inflight_send.len() >= max_in_flight || queue.pending_send.is_empty() {
                executor::timeout(Duration::from_millis(100), queue.wait()).await;
                continue;
            }

            // TODO: Batch as many commands that are immediately available (also need to
            // handle the no_reply case).
            let data = {
                let next_to_send = queue.pending_send.pop_front().unwrap();
                let data = next_to_send.line.clone();
                queue.inflight_send.push_back(next_to_send);
                data
            };

            queue.exit();

            writer.write_all(&data).await?;
        }

        drop(sender_guard);

        Ok(())
    }

    fn cancel_exceeded_deadline(queue: &mut SerialPendingSendQueue) {
        let now = Instant::now();

        // TODO: We need to measure in-flight timeout from the time it was sent to avoid
        // forgetting about it too soon and losing sync.
        if !queue.inflight_send.is_empty() {
            // If the first command is dead, then they are basically all dead
            // since we can't re-sync.

            let send = &queue.inflight_send[0];
            if send.deadline < now {
                for send in queue.inflight_send.drain(..) {
                    send.callback.send(Err(SendCommandError::DeadlineExceeded));
                }

                // TODO: We need to fully error out since the serial port is no
                // longer synced in terms of which command the next reply
                // belongs to.
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
        receiver_guard: ReceiverClosedGuard,
    ) -> Result<()> {
        // Absolute offset of the next received line that needs to be
        let mut next_line_offset = shared.receiver_buffer.last_line_offset().await?;

        /// Stores new axes/capabilities data that should be incorporated into
        /// the state.
        let mut new_state_data = State::default();

        loop {
            let mut buf = [0u8; READ_BUFFER_SIZE];
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                // NOTE: This will be triggered is this is a USB serial device that was
                // disconnected.
                return Err(err_msg("Hit end of the serial read end"));
            }

            let now = Instant::now();
            let now_systime = SystemTime::now();

            // TODO: Consider not erroring out if there are extremely long lines.
            shared.receiver_buffer.append(&buf[0..n], now).await?;

            let config = shared.config.read().await?;

            let mut got_state_change = false;
            new_state_data.capabilities.clear();
            new_state_data.axes.clear();

            let mut command_responses = vec![];

            // Process any newly added lines.
            let end_line_offset = shared.receiver_buffer.last_line_offset().await?;
            while next_line_offset < end_line_offset {
                let line = shared.receiver_buffer.get_line(next_line_offset).await?;
                next_line_offset += 1;

                let mut events = vec![];
                if let Err(e) = parse_response_line(&line.data, &config, &mut events) {
                    eprintln!("Failure parsing response line: {:?}: {}", line.data, e);
                    continue;
                }

                // println!("{:?}", events);

                let mut kind = ReadSerialLogResponse_LineKind::UNKNOWN;

                let mut line_has_state_update = false;
                for event in events {
                    match event {
                        ResponseEvent::Ok => {
                            command_responses.push(Ok(()));
                            kind = ReadSerialLogResponse_LineKind::OK;
                        }
                        ResponseEvent::Error { message } => {
                            command_responses.push(Err(SendCommandError::ReceivedError(message)));
                            kind = ReadSerialLogResponse_LineKind::ERROR;
                        }
                        ResponseEvent::Echo { message, level } => {
                            // TODO: Do something!
                        }
                        ResponseEvent::Capability { name, present } => {
                            new_state_data.capabilities.insert(name, present);
                            line_has_state_update = true;
                        }
                        ResponseEvent::IsStateUpdate => {
                            line_has_state_update = true;
                        }
                        ResponseEvent::AxisValue {
                            id,
                            values,
                            coordinate_system,
                        } => {
                            // Note that since we usually aren't frequently given info on what the
                            // 'Current' coordinate system is, we ignore that info and rely on
                            // clearer offsets.
                            if coordinate_system != CoordinateSystemIndex::Machine {
                                continue;
                            }

                            new_state_data.axes.insert(
                                id,
                                AxisData {
                                    data: TimestampedValue::new(values, line.time),
                                },
                            );
                            line_has_state_update = true;
                        }
                        ResponseEvent::AxisOffset {
                            id,
                            offset,
                            coordinate_system,
                        } => {
                            let idx = match coordinate_system {
                                CoordinateSystemIndex::Specific(idx) => idx,
                                _ => continue,
                            };

                            let mut values = FixedVec::new();
                            values.push(offset);

                            new_state_data
                                .coordinate_systems
                                .entry(idx)
                                .or_insert_with(|| CoordinateSystemData::default())
                                .offset
                                .insert(
                                    id,
                                    AxisData {
                                        data: TimestampedValue::new(values, line.time),
                                    },
                                );

                            line_has_state_update = true;
                        }
                        ResponseEvent::FirmwareState { name } => {
                            new_state_data.firmware_state = TimestampedValue::new(name, line.time);
                            line_has_state_update = true;
                        }
                        ResponseEvent::CurrentToolIndex { index } => {
                            new_state_data.active_tool = TimestampedValue::new(index, line.time);
                            line_has_state_update = true;
                        }
                        ResponseEvent::CurrentCoordinateSystem { index } => {
                            new_state_data.current_coordinate_system =
                                TimestampedValue::new(index, line.time);
                            line_has_state_update = true;
                        }
                        ResponseEvent::CurrentSpindleSpeed(v) => {
                            new_state_data.spindle.current_rpm =
                                TimestampedValue::new(v, line.time);
                            line_has_state_update = true;
                        }
                        ResponseEvent::TargetSpindleSpeed(v) => {
                            new_state_data.spindle.target_rpm = TimestampedValue::new(v, line.time);
                            line_has_state_update = true;
                        }
                        ResponseEvent::SpindleMode(v) => {
                            new_state_data.spindle.mode = TimestampedValue::new(v, line.time);
                            line_has_state_update = true;
                        }
                    }
                }

                if line_has_state_update {
                    kind = ReadSerialLogResponse_LineKind::STATE_UPDATE;
                    got_state_change = true;
                }

                shared
                    .receiver_buffer
                    .set_kind(next_line_offset - 1, kind)
                    .await?;

                // TODO: Delete or move logic to me.
                // Self::process_line(&line);
            }

            if got_state_change {
                // NOTE: If we ended up getting multiple state updates for the
                // same axes in the same batch, we will only record the last
                // update here.
                for (axis, axis_data) in new_state_data.axes.iter() {
                    let axis_config = config
                        .axes_map()
                        .get(axis)
                        .ok_or_else(|| format_err!("Missing axis config: {}", axis))?;

                    if !axis_config.has_collect() {
                        continue;
                    }

                    let streams = shared.axis_metrics.get_or_err(axis)?;

                    for (i, value) in axis_data
                        .data
                        .get()
                        .map(|d| d.as_ref())
                        .unwrap_or(&[])
                        .iter()
                        .cloned()
                        .enumerate()
                    {
                        if axis_config.collect().has_min_value() {
                            if value < axis_config.collect().min_value() {
                                continue;
                            }
                        }

                        let stream = streams
                            .get(i)
                            .ok_or_else(|| err_msg("Wrong number of stream metrics for axis"))?;

                        // TODO: Instead use the axis_data timestamp / the line timestamp.
                        stream.record(now_systime, value).await?;
                    }
                }

                lock!(state <= shared.state.lock().await?, {
                    state.axes.extend(new_state_data.axes.drain());
                    state.capabilities.extend(new_state_data.capabilities.drain());

                    state
                        .firmware_state
                        .insert_if_present(new_state_data.firmware_state.take());

                    state
                        .spindle
                        .current_rpm
                        .insert_if_present(new_state_data.spindle.current_rpm.take());
                    state
                        .spindle
                        .mode
                        .insert_if_present(new_state_data.spindle.mode.take());
                    state
                        .spindle
                        .target_rpm
                        .insert_if_present(new_state_data.spindle.target_rpm.take());

                    state
                        .current_coordinate_system
                        .insert_if_present(new_state_data.current_coordinate_system.take());

                    state
                        .active_tool
                        .insert_if_present(new_state_data.active_tool.take());

                    for (i, mut data) in new_state_data.coordinate_systems.drain() {
                        if let Some(c) = state.coordinate_systems.get_mut(&i) {
                            c.offset.extend(data.offset.drain());
                        }
                    }
                });
            }

            drop(config);

            // NOTE: We only respond to commands after the entire line is processed and all
            // state data is updated to ensure that the caller of the command observes any
            // responses send for the same command (which may be on a previous line or on
            // the same line as the 'ok'.
            if !command_responses.is_empty() {
                // TODO: This must happen after state updates (e.g. responses to commands that
                // were parsed may not get included yet.)

                lock!(queue <= shared.sender_pending_buffer.lock().await?, {
                    for res in command_responses.drain(..) {
                        if let Some(entry) = queue.inflight_send.pop_front() {
                            entry.callback.send(res);
                            queue.notify_all();
                        } else {
                            // TODO: Make this a error after the connection is established.
                            eprintln!("Received response without a command! {:?}", res);
                        }
                    }
                });
            }

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
                    true,
                ));
            }

            // Having non-ascii responses should probably trigger a warning.
        }

        drop(receiver_guard);
    }
}

#[async_trait]
impl ConnectionController for SerialController {

    /// Checks if the controller is fully setup and ready to accept user
    /// commands.
    async fn connected(&self) -> Result<bool> {
        let state = self.shared.state.lock().await?.read_exclusive();
        Ok(state.connected)
    }

    async fn state_proto(&self, proto: &mut MachineStateProto) -> Result<()> {
        let state = self.shared.state.lock().await?.read_exclusive();
        if !state.connected {
            proto.set_connection_state(MachineStateProto_ConnectionState::CONNECTING);
            return Ok(());
        }

        proto.set_connection_state(MachineStateProto_ConnectionState::CONNECTED);

        if let Some(state) = state.firmware_state.get() {
            proto.set_firmware_state(state);
        }

        for (axis_id, axis) in &state.axes {
            let proto = proto.new_axis_values();
            proto.set_id(axis_id);
            if let Some(value) = axis.data.get() {
                proto.value_mut().extend_from_slice(&value[..]);
            }
        }

        if !state.coordinate_systems.is_empty() {
            for i in 0..gcode::STANDARD_COORDINATE_SYSTEMS.len() {
                let proto = proto.new_coordinate_systems();
                proto.set_gcode(gcode::STANDARD_COORDINATE_SYSTEMS[i]);
                if let Some(v) = state.current_coordinate_system.get() {
                    proto.set_current(*v == i as u32 + 1);
                }

                for (axis_id, axis) in &state
                    .coordinate_systems
                    .get(&(i as u32 + 1))
                    .unwrap()
                    .offset
                {
                    let proto = proto.new_offset();
                    proto.set_id(axis_id);
                    if let Some(value) = axis.data.get() {
                        proto.value_mut().extend_from_slice(&value);
                    }
                }
            }
        }

        if let Some(v) = state.spindle.mode.get() {
            proto.spindle_mut().set_mode(*v);
        }

        if let Some(v) = state.spindle.current_rpm.get() {
            proto.spindle_mut().set_current_speed_rpm(*v);
        }

        if let Some(v) = state.spindle.target_rpm.get() {
            proto.spindle_mut().set_target_speed_rpm(*v);
        }

        if let Some(id) = state.active_tool.get() {
            proto.tools_mut().set_active_tool(*id);
        }

        Ok(())
    }

    /// NOTE: This will get the most recently measured value of the axis. It may
    /// be several seconds stale if an update hasn't been received in a while.
    async fn axis_value(&self, axis_name: &str) -> Result<AxisData> {
        let state = self.shared.state.lock().await?.read_exclusive();

        state
            .axes
            .get(axis_name)
            .cloned()
            .ok_or_else(|| err_msg("Missing axis"))
    }

    /// TODO: Make this independent of the SerialController
    ///
    /// CANCEL SAFE
    async fn read_serial_log(
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
                proto.set_value(format_bytes(&line.data));
                proto.set_number(offset);
                proto.set_kind(line.kind);
            }

            response.send(batch).await?;
        }
    }

    /// Explicitly requests state reports from the machine (ignoring if
    /// autoreporting is supported).
    async fn request_state_update(&self) -> Result<()> {
        self.check_clear_to_send().await?;

        // TODO: ALso need grbl support here. Want an immediate update.

        Self::request_state_report_marlin(&self.shared, &AutoReportSupport::default()).await
    }

    async fn set_temperature(&self, axis_id: &str, target: f32) -> Result<()> {
        let config = self.shared.config.read().await?;
        let axis = config.axes_map().get(axis_id).ok_or_else(|| {
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
                format!("M104 T{} S{:.2}\n", num, target)
            } else {
                return Err(err_msg("Unsupported heater id"));
            }
        };

        self.send_command(command, DEFAULT_COMMAND_TIMEOUT).await?;

        Ok(())
    }

    async fn home_x(&self) -> Result<()> {
        self.send_command("G28 X\n", DEFAULT_COMMAND_TIMEOUT).await
    }

    async fn home_y(&self) -> Result<()> {
        self.send_command("G28 Y\n", DEFAULT_COMMAND_TIMEOUT).await
    }

    async fn home_all(&self) -> Result<()> {
        self.send_command("G28 W\n", DEFAULT_COMMAND_TIMEOUT).await
    }

    async fn mesh_level(&self) -> Result<()> {
        self.send_command("G28\n", DEFAULT_COMMAND_TIMEOUT).await
    }

    /// Goes to a 2d position in world coordinates.
    async fn goto(&self, x: f32, y: f32, feed_rate: f32) -> Result<()> {
        let config = self.shared.config.read().await?;

        // Absolute positioning
        self.send_command("G90\n", DEFAULT_COMMAND_TIMEOUT).await?;

        // Switch to world coordinate system (temporarily for just this line).
        let coordinate_system_prefix = {
            if Self::supports_coordinate_systems(&config) {
                "G53 "
            } else {
                ""
            }
        };

        self.send_command(
            format!(
                "{}G0 X{:.2} Y{:.2} F{}\n",
                coordinate_system_prefix, x, y, feed_rate
            ),
            DEFAULT_COMMAND_TIMEOUT,
        )
        .await?;

        Ok(())
    }

    async fn goto3(&self, x: Option<f32>, y: Option<f32>, z: Option<f32>, feed_rate: f32) -> Result<()> {
        let config = self.shared.config.read().await?;

        // Absolute positioning
        self.send_command("G90\n", DEFAULT_COMMAND_TIMEOUT).await?;

        // Switch to world coordinate system (temporarily for just this line).
        let coordinate_system_prefix = {
            if Self::supports_coordinate_systems(&config) {
                "G53 "
            } else {
                ""
            }
        };

        let mut pos_parts = String::new();
        if let Some(v) = x {
            pos_parts.push_str(&format!(" X{:.2}", v));
        }
        if let Some(v) = y {
            pos_parts.push_str(&format!(" Y{:.2}", v));
        }
        if let Some(v) = z {
            pos_parts.push_str(&format!(" Z{:.2}", v));
        }

        self.send_command(
            format!(
                "{}G0{} F{}\n",
                coordinate_system_prefix, pos_parts, feed_rate
            ),
            DEFAULT_COMMAND_TIMEOUT,
        )
        .await?;

        Ok(())
    }

    async fn set_spindle_state(&self, state: &SpindleState) -> Result<()> {
        let config = self.shared.config.read().await?;

        if !config.has_spindle() {
            return Err(
                rpc::Status::invalid_argument("Machine not configured to have a spindle").into(),
            );
        }

        if state.target_speed_rpm() > config.spindle().max_speed_rpm() {
            return Err(
                rpc::Status::invalid_argument("Target spindle RPM higher than max limit").into(),
            );
        }

        let cmd = {
            match state.mode() {
                SpindleState_Mode::OFF => {
                    format!("M5\n")
                }
                SpindleState_Mode::ON_CLOCKWISE | SpindleState_Mode::ON_COUNTERCLOCKWISE => {
                    let code = if state.mode() == SpindleState_Mode::ON_CLOCKWISE {
                        "M3"
                    } else {
                        "M4"
                    };
                    format!("{} S{}\n", code, state.target_speed_rpm())
                }
                _ => return Err(rpc::Status::invalid_argument("Unsupported spindle mode").into()),
            }
        };

        self.send_command(cmd, DEFAULT_COMMAND_TIMEOUT).await?;

        Ok(())
    }

    async fn tool_change(&self, tool_index: i32) -> Result<()> {
        // TODO: Lock the state to prevent concurrent toolchange attempts and wait for
        // dwells to complete successfully before we consider the tool change to
        // be complete.

        let config = self.shared.config.read().await?;

        if !Self::supports_tool_changer(&config) {
            return Err(rpc::Status::invalid_argument(
                "Machine not configured to support tool changing",
            )
            .into());
        }

        self.wait_for_idle().await?;

        // TODO: Wait for any ongoing tool change to finish.

        // TODO: Validate the index.

        let command = {
            if config.firmware() == MachineConfig_Firmware::MARLIN {
                format!("T{}\n", tool_index)
            } else if config.firmware() == MachineConfig_Firmware::PRUSA {
                let mut tool_index = tool_index;

                // Park index
                if tool_index < 0 {
                    tool_index = config.tools().num_slots() as i32;
                }

                // M0: Don't use the tool mapping (always do global operations).
                format!("T{} M0\n", tool_index)
            } else {
                // NOTE: The space is important and Carvera firmware don't seem to work without
                // it.
                format!("M6 T{}\n", tool_index)
            }
        };

        self.send_command(command, DEFAULT_COMMAND_TIMEOUT).await?;

        // On Carvera, we need to wait for the atc state to become ATC_NONE since the
        // tool change command executes many sub-commands.
        if config.firmware() == MachineConfig_Firmware::CARVERA {
            // TODO: Bound this loop's time
            loop {
                let state = self.get_current_axis_value("ATC_STATE").await?;
                let data = state.data.get().ok_or_else(|| err_msg("Missing data"))?;

                if data[0] == 0.0 {
                    break;
                }

                executor::sleep(Duration::from_millis(100)).await?;
            }

            // Wait for firmware to stabilize. Sometimes we see the firmware in the wrong state (e.g. relative instead of absolute positioning mode after a tool change).
            executor::sleep(Duration::from_millis(100)).await?;
        }

        self.wait_for_idle().await
    }

    /// This function basically waits until we have received a state update from
    /// the machine after the start of when the function has been called to
    /// guarantee that the data is consistent relative to any past commands.
    ///
    /// NOTE: THis will eventually terminate since the ReceiverGuard will
    /// eventually disconnect the machine when the receiver thread times out if
    /// new data isn't received for a while.
    async fn get_current_axis_value(&self, axis_id: &str) -> Result<AxisData> {
        let now = Instant::now();

        loop {
            let state = self.shared.state.lock().await?.read_exclusive();
            if !state.connected {
                return Err(rpc::Status::failed_precondition("Machine not connected").into());
            }

            let data = state
                .axes
                .get(axis_id)
                .ok_or_else(|| err_msg("Missing axis data"))?;
            let last_updated = data
                .data
                .last_updated()
                .ok_or_else(|| err_msg("Data missing last update time"))?;

            if last_updated < now {
                drop(state);
                executor::sleep(Duration::from_millis(500)).await?;
                continue;
            }

            return Ok(data.clone());
        }
    }

    async fn wait_for_idle(&self) -> Result<()> {
        let config = self.shared.config.read().await?;

        // TODO: On gRBL style systems, we should just check the machine state and
        // verify it is 'idle'

        // TODO: Send 'M400\n' to wait for all moves to finish
        // (GRBL doesn't support this though and will return ok once commands
        // are completed).

        // ^ May need 2 commands: https://groups.google.com/g/openpnp/c/X3tj8LStGvU

        // Or use 'G4P0' command: https://groups.google.com/g/openpnp/c/EcA5NqzT9BI

        match config.firmware() {
            MachineConfig_Firmware::UNKNOWN
            | MachineConfig_Firmware::GENERIC
            | MachineConfig_Firmware::GRBL => {
                for i in 0..2 {
                    self.send_command("G4 P0\n", DEFAULT_COMMAND_TIMEOUT)
                        .await?;
                }
            }
            MachineConfig_Firmware::MARLIN
            | MachineConfig_Firmware::PRUSA
            | MachineConfig_Firmware::SMOOTHIEWARE
            | MachineConfig_Firmware::CARVERA
            | MachineConfig_Firmware::KLIPPER => {
                self.send_command("M400\n", DEFAULT_COMMAND_TIMEOUT).await?;
            }
            MachineConfig_Firmware::CNC_CONTROLLER => return Err(err_msg("Firmware doesn't support serial"))
        }

        Ok(())
    }

    async fn send_command_impl(&self, line: Bytes, timeout: Duration) -> Result<()> {
        self.check_clear_to_send().await?;

        // TODO: Need to verify that the line ends in a '\n' and has only one of them.

        Self::send_command_inner(&self.shared, line, timeout, SendCommandFlags::empty()).await?;
        Ok(())
    }

    async fn enqueue_command_impl(
        &self,
        line: Bytes,
        timeout: Duration,
    ) -> Result<PendingCommand> {
        self.check_clear_to_send().await?;

        let cmd =
            Self::enqueue_command_inner(&self.shared, line, timeout, SendCommandFlags::empty())
                .await?;
        Ok(cmd)
    }

    async fn full_stop(&self) -> Result<()> {
        // TODO: Also implement reset_using_dtr in parallel to this (wait for both
        // futures to complete regardless of success).

        let config = self.shared.config.read().await?;

        // "Soft-Reset" GRBL realtime command.
        if config.firmware() == MachineConfig_Firmware::GRBL
            || config.firmware() == MachineConfig_Firmware::SMOOTHIEWARE
            || config.firmware() == MachineConfig_Firmware::CARVERA
        {
            Self::send_command_inner(
                &self.shared,
                (&b"\x18"[..]).into(),
                Duration::from_secs(120),
                SendCommandFlags::SKIP_LINE
                    | SendCommandFlags::STOP_AFTER
                    | SendCommandFlags::NO_REPLY,
            )
            .await?;

            return Ok(());
        }

        // Skips all other entries in line and after stops any command from running
        // after this one.
        Self::send_command_inner(
            &self.shared,
            "M112\n".into(),
            Duration::from_secs(120),
            SendCommandFlags::SKIP_LINE | SendCommandFlags::STOP_AFTER,
        )
        .await?;

        Ok(())
    }

}
