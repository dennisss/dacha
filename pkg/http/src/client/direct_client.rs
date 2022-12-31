use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::time::Duration;
use std::time::Instant;

use common::condvar::Condvar;
use common::errors::*;
use common::io::{Readable, Writeable};
use executor::channel;
use executor::child_task::ChildTask;
use executor::sync::Mutex;
use net::backoff::*;
use net::tcp::TcpStream;
use parsing::ascii::AsciiString;

use crate::alpn::*;
use crate::client::client_interface::*;
use crate::client::resolver::ResolvedEndpoint;
use crate::connection_event_listener::{ConnectionEventListener, ConnectionShutdownDetails};
use crate::header::*;
use crate::method::*;
use crate::request::*;
use crate::response::Response;
use crate::response_channel::*;
use crate::uri::*;
use crate::{v1, v2};

/*
TODO:
A server MUST NOT switch protocols unless the received message semantics can be honored by the new protocol

Key details about an upgrade request:
- We shouldn't send any more requests on a connection which is in the process of being upgraded.
    - This implies that we should know if we're upgrading
*/

#[derive(Clone)]
pub struct DirectClientOptions {
    /// If present, use these options to connect with SSL/TLS. Otherwise, we'll
    /// send requests over plain text.
    pub tls: Option<crypto::tls::ClientOptions>,

    /// If true, we'll immediately connect using HTTP2 and fail if it is not
    /// supported by the server. By default (when this is false), we'll start by
    /// sending HTTP1 requests until we are confident that the remote server
    /// supports HTTP2.
    pub force_http2: bool,

    /// If true, we will attempt to upgrade insecure HTTPv1 connections to v2.
    /// If the first connection attempts fails to upgrade, we will assume that
    /// the backend only supports v1 in the future and we won't attempt to
    /// upgrade on future attempts.
    ///
    /// TODO: Fully implement this.
    pub upgrade_plaintext_http2: bool,

    /// Maximum number of individual connections (network sockets) we will open.
    ///
    /// - For HTTP1 backends, connection balancing is performed as follows:
    ///   - If an idle connection is open, use that.
    ///   - Else if we are below max_num_connections, create a new channel and
    ///     use that.
    ///   - Else while below 'http1_max_requests_per_connection' use the
    ///     connection with the fewest active requests.
    ///   - Else block for some amount of time and repeat this procedure.
    /// - For HTTP2 backends, we will always maintain one non-shutdown
    ///   connection used as the primary sending connection and some number of
    ///   connections still being cleaned up.
    ///
    /// In the case that the connection pool contains both active V1/V2
    /// connections, the active V2 connection is preferred for new requests and
    /// other connections are ignored.
    pub max_num_connections: usize,

    /// For HTTP1 backends, this is the maximum number of concurrent requests
    /// that can be sent on a single connection. A value > 1 implies pipelining.
    ///
    /// For HTTP2, this setting is ignored as it is controlled in the HTTP2
    /// settings.
    ///
    /// TODO: In HTTP2 if there a good reason to maybe add additional queueing
    pub http1_max_requests_per_connection: usize,

    /// Backoff parameters for limiting the speed of retrying connecting to the
    /// remote server after a failure has occured.
    pub connection_backoff: ExponentialBackoffOptions,

    /// Max amount of time to step on establishing the connection (per connect
    /// attempt). This mainly includes the TCP acknowledge and TLS handshake.
    pub connect_timeout: Duration,

    /// Time after the last request (or creation of the client) at which we will
    /// shut down any active connections. Requests made later are still allowed
    /// but will be delayed by the need to re-connect.
    ///
    /// Setting this to a non-zero value means that we should try to keep a
    /// connection open even when no request is active.
    ///
    /// NOTE: This applies per-connection to allow for the HTTP1 connection to
    /// shrink over time.
    ///
    /// TODO: Suppose we are currently starting up a server which has many
    /// dependent backends which all take a while to connect. We should support
    /// holding off on respecting the idle timeout until the server is actually
    /// completely up to avoid the first request to the server getting hit with
    /// the connection delays.
    ///
    /// TODO: If this is zero or close to zero then consider marking HTTP1
    /// requests with 'Connection: close'.
    pub idle_timeout: Duration,

    /// Maximum number of requests allowed to be incomplete across all
    /// connections.
    ///
    /// When we are below this limit but above the
    /// max_num_connections/http1_max_requests_per_connection limits,
    /// requests will wait for an available connection. Above this limit,
    /// requests will be instantly rejected.
    ///
    /// NOTE: This is meant to be primarily a mechanism for protecting the
    /// backend from overload rather than protecting the client.
    pub max_outstanding_requests: usize,

    /// If true, if the remote backend gracefully shuts down the connection
    /// (e.g. with an HTTP2 GOAWAY with NO_ERROR), then this will be tried as a
    /// failure which requires backoff before reconnecting.
    ///
    /// If false, then we are allowed to immediately reconnect after graceful
    /// shutdowns.
    ///
    /// Set this to true if you think the backend represents a
    /// single server completely stopping and won't be available for a while.
    /// Hopefully be the time the backoff is complete, the LoadBalancedClient
    /// could use other signals to un-list this DirectClient instance from
    /// the set of available backends.
    pub remote_shutdown_is_failure: bool,

    /// If true, then while a request's completion (or the start time of the
    /// client) occured within idle_timeout, we will ensure that at least one
    /// connection is connected to the backend.
    ///
    /// If false, we will only begin connections if a request is actively
    /// waiting to be executed.
    pub eagerly_connect: bool,
}

/// An HTTP client which is just responsible for connecting to a single ip
/// address and port.
///
/// - This supports both HTTP v1 and v2 connections.
/// - A pool of connections is internally maintained (primarily to HTTP v1).
/// - When a connection fails, it will attempt to re-establish the connection
///   with the same settings.
/// - Re-establishing a connection will be done using a timing based backoff
///   mechanism.
///
/// NOTE: After a DirectClient is created with ::new(), a copy of it should be
/// scheduled to run in the background with ::run().
///
/// TODO: When the client is dropped, shut down all connections as it will no
/// longer be possible to start new connections but existing connections may
/// still be running.
#[derive(Clone)]
pub struct DirectClient {
    shared: Arc<Shared>,
}

struct Shared {
    endpoint: ResolvedEndpoint,

    options: DirectClientOptions,

    /// Overall state of this client.
    ///
    /// This should always be the value of next_overall_state() after all events
    /// in recieved_events have been incorporated into the processing_state.
    ///
    /// NOTE: You MUST lock received_events in order to update this value to
    /// avoid there being unprocessed events which haven't been incorporated
    /// from the processing_state.
    overall_state: Condvar<ClientState>,

    /// Recently received events from connections or other places which have not
    /// yet been incorporated into processing_state.
    received_events: Condvar<ReceivedEvents>,

    /// State tracking all active connections and requests.
    ///
    /// This MUST be locked before overall_state/received_events if locking
    /// multiple objects.
    processing_state: Mutex<State>,

    event_listener: Arc<dyn ClientEventListener>,
}

#[derive(Default)]
struct ReceivedEvents {
    connection_events: HashMap<usize, ConnectionEvents>,

    /// Set of connections which have recently finished turning up.
    /// The entry may be None if a failure occured while connectinf.
    connection_opened: Vec<(usize, Option<ConnectionEntry>)>,

    /// TODO: Implement the setting of this.
    http1_non_persistent_connections: bool,

    /// Used to indicate that something else interesting happened (like a
    /// request getting enqueued) that requires us to revisit the state.
    wakeup: bool,
}

impl ReceivedEvents {
    fn is_empty(&self) -> bool {
        self.connection_events.is_empty()
            && self.connection_opened.is_empty()
            && !self.http1_non_persistent_connections
            && !self.wakeup
    }
}

#[derive(Default)]
struct ConnectionEvents {
    num_completed_requests: usize,
    failed: bool,
    closed: bool,
    shutting_down: bool,
}

struct State {
    running: bool,

    shutting_down: bool,

    failing: bool,

    /// Requests which have been given to the client but have not yet been
    /// assigned to a connection to be run.
    unassigned_requests: VecDeque<ClientLocalRequest>,

    /// Map from connection id to data for that connection.
    connection_pool: HashMap<usize, ConnectionEntry>,

    /// If true, then we have detected that the backend only supports HTTP1.x
    /// and does not like to persist connections for longer than one request.
    ///
    /// This essentially overrides idle_timeout to 0 when processing HTTP1
    /// connections.
    http1_non_persistent_connections: bool,

    /// Overall last active time across all connections.
    last_active: Instant,
}

impl State {
    /// Total number of outstanding requests across all connections.
    fn num_outstanding_requests(&self) -> usize {
        let mut n = self.unassigned_requests.len();
        for (_, conn) in &self.connection_pool {
            n += conn.num_outstanding_requests;
        }

        n
    }
}

struct ClientLocalRequest {
    request: Request,
    request_context: ClientRequestContext,
    response_sender: ResponseSender,
}

struct ConnectionEntry {
    num_outstanding_requests: usize,

    /// Time at which the last request on this connection finished running.
    last_active: Instant,

    /// If true, this connection is being shut down
    shutting_down: bool,

    is_secure: bool,

    instance: ConnectionInstance,

    main_task: ChildTask,
}

enum ConnectionInstance {
    V1(v1::ClientConnection),
    V2(v2::Connection),
}

impl DirectClient {
    pub fn new(
        endpoint: ResolvedEndpoint,
        mut options: DirectClientOptions,
        event_listener: Arc<dyn ClientEventListener>,
    ) -> Self {
        if let Some(tls_options) = &mut options.tls {
            if let Host::Name(name) = &endpoint.authority.host {
                tls_options.hostname = name.clone();
            }
            tls_options.alpn_ids.push(ALPN_HTTP2.into());
            tls_options.alpn_ids.push(ALPN_HTTP11.into());
        }

        Self {
            shared: Arc::new(Shared {
                endpoint,
                options,
                overall_state: Condvar::new(ClientState::Idle),
                received_events: Condvar::new(ReceivedEvents::default()),
                processing_state: Mutex::new(State {
                    running: true,
                    shutting_down: false,
                    failing: false,
                    unassigned_requests: VecDeque::new(),
                    connection_pool: HashMap::new(),
                    http1_non_persistent_connections: false,
                    last_active: Instant::now(),
                }),
                event_listener,
            }),
        }
    }

    pub async fn shutdown(&self) {
        let mut state = self.shared.processing_state.lock().await;
        state.shutting_down = true;
        drop(state);

        self.shared.received_events.lock().await.notify_all();
    }

    /// Main loop of the client. This should be called in a dedicated task by
    /// the creater of the DirectClient.
    pub fn run(&self) -> impl Future<Output = ()> {
        let shared = self.shared.clone();
        return async move { DirectClientRunner::new(shared).run().await };
    }
}

#[async_trait]
impl ClientInterface for DirectClient {
    async fn request(
        &self,
        mut request: Request,
        request_context: ClientRequestContext,
    ) -> Result<Response> {
        // TODO: We should allow the Connection header, but we shouldn't allow any
        // options which are used internally (keep-alive and close)
        for header in &request.head.headers.raw_headers {
            if header.is_transport_level() {
                return Err(format_err!(
                    "Request contains reserved header: {}",
                    header.name.as_str()
                ));
            }
        }

        let mut state = self.shared.processing_state.lock().await;

        if !state.running
            || state.shutting_down
            || state.num_outstanding_requests() >= self.shared.options.max_outstanding_requests
        {
            return Err(crate::v2::ProtocolErrorV2 {
                code: crate::proto::v2::ErrorCode::REFUSED_STREAM,
                local: true,
                message: "Client not accepting additional requests.".into(),
            }
            .into());
        }

        let (response_sender, response_receiver) = new_response_channel();

        state.unassigned_requests.push_back(ClientLocalRequest {
            request,
            request_context,
            response_sender,
        });

        // Notify the runner thread to take a look (and possibly shut down the queue).
        {
            let mut events = self.shared.received_events.lock().await;
            events.wakeup = true;

            if state.unassigned_requests.len() >= self.shared.options.max_outstanding_requests {
                let mut overall_state = self.shared.overall_state.lock().await;
                *overall_state = ClientState::Failure;
                overall_state.notify_all();
            }

            events.notify_all();
        }

        drop(state);

        response_receiver.recv().await
    }

    async fn current_state(&self) -> ClientState {
        *self.shared.overall_state.lock().await
    }
}

struct DirectClientRunner {
    shared: Arc<Shared>,

    /// Id of the last connection we've tried starting.
    last_connection_id: usize,

    /// NOTE: There is always an attempt active in this backoff.
    connect_backoff: ExponentialBackoff,

    /// If we are currently in a Failure state, this is the time at which we can
    /// reset the state to Connecting. Note that we are only allowed to create a
    /// new connection while in a non-Failure state.
    current_connect_backoff_end: Option<Instant>,

    /// If true, the most recent value of current_connect_backoff_end was
    /// generated by a connection failure. This is true until the timeout
    /// elapses and during that time we won't increase the backoff factor any
    /// more.
    in_failure_backoff: bool,

    /// Maximum number of connections which can be in a 'connecting' state at a
    /// given point in time. Starts at 1 and doubles with each successful
    /// connection opened. Resets to 1 whenever any failure is detected.
    max_concurrent_connecting: usize,

    /// In-progress tasks we are using to connect to the backend.
    connecting_tasks: HashMap<usize, ChildTask>,

    /// The next time at which something important occurs (e.g. a timeout
    /// elapses).
    next_eventful_time: Instant,
}

impl DirectClientRunner {
    fn new(shared: Arc<Shared>) -> Self {
        let mut connect_backoff =
            ExponentialBackoff::new(shared.options.connection_backoff.clone());
        let _ = connect_backoff.start_attempt(); // Will always be 'no backoff'.

        Self {
            shared,
            last_connection_id: 0,
            connect_backoff,
            current_connect_backoff_end: None,
            max_concurrent_connecting: 1,
            connecting_tasks: HashMap::new(),
            next_eventful_time: Instant::now() + Duration::from_secs(10),
            in_failure_backoff: false,
        }
    }

    async fn run(&mut self) {
        let shared = self.shared.clone();

        loop {
            let mut state = shared.processing_state.lock().await;

            {
                let mut events = shared.received_events.lock().await;
                self.process_events(&mut state, &mut events);

                let next_state = self.next_overall_state(&mut state);

                let mut overall_state = self.shared.overall_state.lock().await;
                if next_state != *overall_state {
                    *overall_state = next_state;
                    overall_state.notify_all();
                }
            }

            // When shutting down, wait for all connections to finish running.
            // TODO: What about requests still being started during the suht down.
            if state.shutting_down {
                if state.connection_pool.is_empty() {
                    break;
                }

                for (_, conn) in &mut state.connection_pool {
                    if !conn.shutting_down {
                        conn.shutting_down = true;
                        match &conn.instance {
                            ConnectionInstance::V1(c) => c.shutdown(),
                            ConnectionInstance::V2(c) => c.shutdown(true).await,
                        }
                    }
                }

                drop(state);
                executor::sleep(Duration::from_millis(500)).await;
                continue;
            }

            self.process_unassigned_requests(&mut state).await;

            // Respect idle timeouts.
            // NOTE: To support idle timeouts of 0, this must be after
            // process_unassigned_requests.
            self.process_idle_connections(&mut state).await;

            self.perform_eager_connecting(&mut state);

            // Wait for something to happen.
            // TODO: Have a timeout on this to handle idleness and so on..
            {
                let events = shared.received_events.lock().await;
                if !events.is_empty() {
                    continue;
                }

                // It's possible that all our changes to connections/requests
                {
                    let next_state = self.next_overall_state(&mut state);
                    let mut overall_state = self.shared.overall_state.lock().await;
                    if next_state != *overall_state {
                        *overall_state = next_state;
                        overall_state.notify_all();
                    }
                }

                drop(state);

                let wait_time = {
                    let now = Instant::now();
                    (if self.next_eventful_time < now {
                        Duration::from_secs(10)
                    } else {
                        self.next_eventful_time - now
                    }) + Duration::from_micros(100)
                }
                .min(Duration::from_secs(10));

                executor::timeout(wait_time, events.wait(())).await;
            }
        }

        {
            let mut state = self.shared.processing_state.lock().await;
            state.running = false;

            while let Some(request) = state.unassigned_requests.pop_front() {
                request
                    .response_sender
                    .send(Err(crate::v2::ProtocolErrorV2 {
                        code: crate::proto::v2::ErrorCode::REFUSED_STREAM,
                        local: true,
                        message: "DirectClient shutting down.".into(),
                    }
                    .into()))
                    .await;
            }
        }
    }

    fn set_next_eventful_time(&mut self, time: Instant) {
        let now = Instant::now();
        if self.next_eventful_time < now || time < self.next_eventful_time {
            self.next_eventful_time = time;
        }
    }

    fn process_events(&mut self, state: &mut State, events: &mut ReceivedEvents) {
        if events.http1_non_persistent_connections {
            state.http1_non_persistent_connections = true;
            events.http1_non_persistent_connections = false; // Ack the event.
        }

        events.wakeup = false; // Ack the event.

        for (connection_id, change) in events.connection_events.drain() {
            // NOTE: Any requests sent while the connection is still connecting (e.g.
            // Upgrades will be ignored in our request count).
            if let Some(conn) = state.connection_pool.get_mut(&connection_id) {
                conn.num_outstanding_requests -= change.num_completed_requests;

                if change.shutting_down {
                    conn.shutting_down = true;
                }

                // Update last_active time when a request is completed (shutting down a
                // connection with active connections also stops all of its requests).
                // NOTE: When shutting down, the request completion event may not occur.
                if change.num_completed_requests != 0
                    || ((change.shutting_down || change.closed)
                        && conn.num_outstanding_requests > 0)
                {
                    conn.last_active = Instant::now();
                    state.last_active = conn.last_active;
                }
            }

            if change.failed {
                // Mark the next time at which we can connect to new stuff
                // and reset connection parallel.
                self.mark_failure(state);
            }

            if change.closed {
                state.connection_pool.remove(&connection_id);

                // Handle the case of the connection closing before it is added to the pool (see
                // also the next for loop).
                self.connecting_tasks.remove(&connection_id);
            }
        }

        // NOTE: This must run after the connection_events to clean up any connection
        // events that occured before the connection was fully established.
        for (connection_id, entry) in events.connection_opened.drain(0..) {
            if let None = self.connecting_tasks.remove(&connection_id) {
                // Connection closed before being completely added to the pool.
                // We will never get a second closed event so we can't add it to the pool.
                continue;
            }

            if let Some(entry) = entry {
                state.connection_pool.insert(connection_id, entry);

                // Assuming we are not in a failure state, we can increase the
                // max_concurrent_connecting
                if !self.current_connect_backoff_end.is_some() {
                    self.max_concurrent_connecting = (2 * self.max_concurrent_connecting)
                        .min(self.shared.options.max_num_connections);
                }
            } else {
                self.mark_failure(state);
            }
        }

        // Reset failing state once the backoff has been exceeded.
        if state.failing {
            if let Some(backoff_end) = self.current_connect_backoff_end.clone() {
                if Instant::now() >= backoff_end {
                    state.failing = false;
                } else {
                    self.set_next_eventful_time(backoff_end);
                }
            } else {
                state.failing = false;
            }
        }
    }

    /// NOTE: This does not need to transition overall_state to the Failure
    /// state as the code from which the failure propagated should do that
    /// immediately (to avoid processing delay).
    fn mark_failure(&mut self, state: &mut State) {
        state.failing = true;

        // Suppress failures while a backoff is already in progress (e.g. if we have
        // more than one connection, they may both fail around the same time).
        //
        // NOTE: 'current_connect_backoff_end' may still not be None if
        // !in_failure_backoff as the backoff doesn't cool down to zero after the first
        // success.
        //
        // TODO: Consider instead waiting the backoff duration since the last failure
        // rather than the first.
        if self.in_failure_backoff {
            return;
        }

        self.in_failure_backoff = true;

        self.connect_backoff.end_attempt(false);

        let dur = match self.connect_backoff.start_attempt() {
            ExponentialBackoffResult::Start => Duration::from_micros(1),
            ExponentialBackoffResult::StartAfter(duration) => duration,
            ExponentialBackoffResult::Stop => Duration::from_secs(600),
        };

        self.current_connect_backoff_end = Some(Instant::now() + dur);

        // Reset
        self.max_concurrent_connecting = 1;
    }

    /// Attempt to assign requests to channels.
    async fn process_unassigned_requests(&mut self, state: &mut State) {
        let mut num_reserved_connections = 0;

        let http1_per_connection_request_limit = if state.http1_non_persistent_connections {
            1
        } else {
            self.shared.options.http1_max_requests_per_connection
        };

        // NOTE: We must always go through all unassigned_requests just in case we need
        // to remove any which don't have wait_for_ready

        let mut request_i = 0;
        while request_i < state.unassigned_requests.len() {
            // TODO: While checking wait_for_ready, do not count issues with us being at the
            // queuing limit as the request is already queued successfully in the
            // DirectClient.

            let mut found_v1 = false;
            let mut found_v2 = false;
            let mut min_outstanding_requests = usize::MAX;

            let mut best_connection_id = None;

            // Find an existing connection which isn't shutdown to use.
            for (connection_id, connection) in &state.connection_pool {
                // NOTE: accepting_requests() only determines if it is shutting down and doesn't
                // judge queue limits.
                match &connection.instance {
                    ConnectionInstance::V1(c) => {
                        found_v1 = true;
                        if !found_v2
                            && c.accepting_requests()
                            && connection.num_outstanding_requests < min_outstanding_requests
                        {
                            best_connection_id = Some(*connection_id);
                            min_outstanding_requests = connection.num_outstanding_requests;
                        }
                    }
                    ConnectionInstance::V2(c) => {
                        // When we have HTTP2 connections, don't use any V1 connections even if
                        // active.
                        if !found_v2 {
                            best_connection_id = None;
                        }

                        found_v2 = true;

                        if c.accepting_requests().await {
                            best_connection_id = Some(*connection_id);
                            min_outstanding_requests = connection.num_outstanding_requests;
                            break;
                        }
                    }
                }
            }

            // If we found a connection and it isn't full, use it.
            if let Some(connection_id) = best_connection_id {
                let can_use = {
                    if found_v2 {
                        // TODO: Check the internal HTTP2 connection's queue length.

                        true
                    } else {
                        // V1 connection : Only use the connection if it is completely idle or we
                        // are not allowed to make more connections.
                        min_outstanding_requests == 0
                            || (state.connection_pool.len()
                                >= self.shared.options.max_num_connections
                                && min_outstanding_requests < http1_per_connection_request_limit)
                    }
                };

                if can_use {
                    let request_entry = state.unassigned_requests.remove(request_i).unwrap();
                    self.start_requesting(request_entry, connection_id, state)
                        .await;
                }

                continue;
            }

            if num_reserved_connections < self.connecting_tasks.len() {
                // 'Reserve' one of the connecting connections to this request for future use.
                num_reserved_connections += 1;
            } else if
            // In HTTP2 mode, only allow adding one more connection if the current connection is
            // shut down. TODO: Move this to try_start_connection
            (!found_v2 || self.connecting_tasks.len() == 0)
                && self.try_start_connection(state)
            {
                // Reserve the newly started connection for future use by this request.
                num_reserved_connections += 1;
            } else if state.failing
                && !state.unassigned_requests[request_i]
                    .request_context
                    .wait_for_ready
            {
                let entry = state.unassigned_requests.remove(request_i).unwrap();

                entry
                    .response_sender
                    .send(Err(crate::v2::ProtocolErrorV2 {
                        code: crate::proto::v2::ErrorCode::REFUSED_STREAM,
                        local: true,
                        message: "Client not ready.".into(),
                    }
                    .into()))
                    .await;
                continue;
            }

            request_i += 1;
        }
    }

    /// Returns whether or not a connection was actually scheduled.
    fn try_start_connection(&mut self, state: &mut State) -> bool {
        if (state.connection_pool.len() + self.connecting_tasks.len())
            >= self.shared.options.max_num_connections
            || self.connecting_tasks.len() >= self.max_concurrent_connecting
        {
            return false;
        }

        if let Some(backoff_end) = self.current_connect_backoff_end.clone() {
            if Instant::now() < backoff_end {
                self.set_next_eventful_time(backoff_end);
                return false;
            }
        }

        // When all connections have idled out, only allow starting up one connection
        // (as we don't know if we should use HTTP2 or now).
        if state.connection_pool.is_empty() && !self.connecting_tasks.is_empty() {
            return false;
        }

        self.current_connect_backoff_end = {
            self.connect_backoff.end_attempt(true);
            match self.connect_backoff.start_attempt() {
                ExponentialBackoffResult::Start => None,
                ExponentialBackoffResult::StartAfter(duration) => Some(Instant::now() + duration),
                ExponentialBackoffResult::Stop => Some(Instant::now() + Duration::from_secs(600)),
            }
        };
        self.in_failure_backoff = false;

        // TODO: Given that the last backoff is over, we can now mark the overall_state
        // as connecting or something else.

        let connection_id = self.last_connection_id + 1;
        self.last_connection_id = connection_id;

        println!(
            "[http::Client] Starting new connection with id {}",
            connection_id
        );

        self.connecting_tasks.insert(
            connection_id,
            ChildTask::spawn(Self::new_connection(self.shared.clone(), connection_id)),
        );

        true
    }

    /// Shuts down all connections which haven't been active in a while.
    async fn process_idle_connections(&mut self, state: &mut State) {
        let now = Instant::now();

        for (connection_id, conn) in &mut state.connection_pool {
            if conn.shutting_down || conn.num_outstanding_requests > 0 {
                continue;
            }

            if conn.last_active + self.shared.options.idle_timeout >= now {
                self.set_next_eventful_time(conn.last_active + self.shared.options.idle_timeout);
                continue;
            }

            println!(
                "[http::Client] Shutting down idle connection: {}",
                *connection_id
            );

            conn.shutting_down = true;
            match &conn.instance {
                ConnectionInstance::V1(c) => c.shutdown(),
                ConnectionInstance::V2(c) => c.shutdown(true).await,
            }
        }
    }

    fn perform_eager_connecting(&mut self, state: &mut State) {
        let mut now = Instant::now();

        if !self.shared.options.eagerly_connect
            || !self.connecting_tasks.is_empty()
            || state.http1_non_persistent_connections
        {
            return;
        }

        // When we are idle, don't perform eager connecting (otherwise we will get into
        // an infinite loop of opening a connection, idle closing it, etc.)
        if state.last_active + self.shared.options.idle_timeout < now {
            self.set_next_eventful_time(state.last_active + self.shared.options.idle_timeout);
            return;
        }

        // Check if we have at least one active connection.
        for (_, conn) in &state.connection_pool {
            if !conn.shutting_down {
                return;
            }
        }

        self.try_start_connection(state);
    }

    fn next_overall_state(&self, state: &mut State) -> ClientState {
        if state.shutting_down {
            return ClientState::Shutdown;
        }

        // TODO: Unless
        if !state.running
            || state.num_outstanding_requests() >= self.shared.options.max_outstanding_requests
            || state.failing
        {
            return ClientState::Failure;
        }

        for (_, conn) in &state.connection_pool {
            // Have at least one connection that is not shut down.
            if !conn.shutting_down {
                return ClientState::Ready;
            }
        }

        if !self.connecting_tasks.is_empty() {
            return ClientState::Connecting;
        }

        ClientState::Idle
    }

    /// Attempts to create a new connection to the client's backend.
    async fn new_connection(shared: Arc<Shared>, connection_id: usize) {
        // TODO: Measure and record how long it takes to establish a connection.
        let mut start_time = Instant::now();
        let entry = executor::timeout(
            shared.options.connect_timeout.clone(),
            Self::new_connection_inner(&shared, connection_id),
        )
        .await;

        let value = match entry {
            Ok(Ok(entry)) => Some(entry),
            Err(_) => {
                eprintln!("[http::Client] Timed out while connecting");
                None
            }
            Ok(Err(e)) => {
                eprintln!("[http::Client] Failure while connecting: {}", e);
                None
            }
        };

        let mut end_time = Instant::now();

        let connection_version = match &value {
            Some(entry) => match &entry.instance {
                ConnectionInstance::V1(_) => "V1",
                ConnectionInstance::V2(_) => "V2",
            },
            None => "",
        };

        println!(
            "[http::Client] Connect {} took: {:?}",
            connection_version,
            end_time - start_time
        );

        let mut events = shared.received_events.lock().await;
        events.connection_opened.push((connection_id, value));
        events.notify_all();
    }

    /// NOTE: Must be called with a lock on the connection pool to ensure that
    /// no one else is also making one at the same time.
    async fn new_connection_inner(
        shared: &Arc<Shared>,
        connection_id: usize,
    ) -> Result<ConnectionEntry> {
        // Ways in which this can fail:
        // - io::ErrorKind::ConnectionRefused: REached the server but it's not serving
        //   on the given port.
        let mut raw_stream = TcpStream::connect(shared.endpoint.address.clone()).await?;
        raw_stream.set_nodelay(true)?;

        let (mut reader, mut writer) = raw_stream.split();

        let mut start_http2 = shared.options.force_http2;

        let mut is_secure = false;

        if let Some(client_options) = &shared.options.tls {
            is_secure = true;

            let mut tls_client = crypto::tls::Client::new();

            // TODO: Include a timeout in this as well (or make the connect_timeout cover
            // this entire function).
            let tls_stream = tls_client.connect(reader, writer, client_options).await?;

            // TODO: Save handshake info so that the user can access it.

            reader = Box::new(tls_stream.reader);
            writer = Box::new(tls_stream.writer);

            if let Some(protocol) = tls_stream.handshake_summary.selected_alpn_protocol {
                if protocol.as_ref() == ALPN_HTTP2.as_bytes() {
                    start_http2 = true;
                }
            }
        }

        let event_listener = Box::new(ConnectionListener {
            connection_id,
            shared: Arc::downgrade(shared),
        });

        if start_http2 {
            let connection_options = v2::ConnectionOptions::default();

            let connection_v2 = v2::Connection::new(connection_options, None);
            connection_v2.set_event_listener(event_listener).await;

            let initial_state = v2::ConnectionInitialState::raw();

            let runner = connection_v2.run(initial_state, reader, writer);

            let main_task = ChildTask::spawn(Self::connection_runner(
                Arc::downgrade(shared),
                connection_id,
                runner,
            ));

            // TODO: Wait for the receiving remote headers.

            return Ok(ConnectionEntry {
                last_active: Instant::now(),
                is_secure,
                instance: ConnectionInstance::V2(connection_v2),
                main_task,
                num_outstanding_requests: 0,
                shutting_down: false,
            });
        }

        let conn = v1::ClientConnection::new();
        conn.set_event_listener(event_listener).await?;

        let main_task = ChildTask::spawn(Self::connection_runner(
            Arc::downgrade(shared),
            connection_id,
            conn.run(reader, writer),
        ));

        // Attempt to upgrade to HTTP2 over clear text.
        if !shared.options.tls.is_some() && shared.options.upgrade_plaintext_http2 {
            let local_settings = crate::v2::SettingsContainer::default();

            let mut connection_options = vec![];
            connection_options.push(crate::headers::connection::ConnectionOption::Unknown(
                parsing::ascii::AsciiString::from("Upgrade").unwrap(),
            ));

            // TODO: Copy the host and uri from the first request?
            let upgrade_uri = Uri {
                scheme: None,
                authority: Some(shared.endpoint.authority.clone()),
                path: AsciiString::from("/").unwrap(),
                query: None,
                fragment: None,
            };

            let mut upgrade_request = RequestBuilder::new()
                // HEAD is used to avoid having to read a response body if we end up
                .method(Method::HEAD)
                .uri(upgrade_uri)
                .header(CONNECTION, "Upgrade, HTTP2-Settings")
                .header("Upgrade", "h2c")
                .build()
                .unwrap();

            local_settings
                .append_to_request(&mut upgrade_request.head.headers, &mut connection_options);

            // TODO: Explicitly enqueue the requests. If the connection dies but we never
            // started sending the reuqest, then we can immediately re-try it.
            let res = conn.enqueue_request(upgrade_request).await?.await?;

            // TODO: Record this decision so that we don't re-attempt to
            let res = match res {
                v1::ClientConnectionResponse::Regular { response } => {
                    println!("{:?}", response.head);
                    println!("DID NOT UPGRADE")
                }
                v1::ClientConnectionResponse::Upgrading { response_head, .. } => {
                    return Err(err_msg("UPGRADING"));
                }
            };
        }

        Ok(ConnectionEntry {
            last_active: Instant::now(),
            is_secure,
            instance: ConnectionInstance::V1(conn),
            main_task,
            num_outstanding_requests: 0,
            shutting_down: false,
        })
    }

    // NOTE: This uses a Weak pointer to ensure that the ClientShared and Connection
    // can be dropped which may lead to the Connection shutting down.
    async fn connection_runner<F: std::future::Future<Output = Result<()>>>(
        client_shared: Weak<Shared>,
        connection_id: usize,
        f: F,
    ) {
        // TODO: Limit the logging rate of this.
        if let Err(e) = f.await {
            eprintln!("[http::Client] Connection failed: {:?}", e);
        }

        if let Some(client_shared) = client_shared.upgrade() {
            // NOTE: The shutdown event should always be called before this to trigger the
            // transition to a failing state.
            let mut events = client_shared.received_events.lock().await;
            events
                .connection_events
                .entry(connection_id)
                .or_default()
                .closed = true;

            events.notify_all();
        }
    }

    async fn start_requesting(
        &self,
        mut request_entry: ClientLocalRequest,
        connection_id: usize,
        state: &mut State,
    ) {
        let mut conn = state.connection_pool.get_mut(&connection_id).unwrap();

        let request = request_entry.request;
        let response_sender = request_entry.response_sender;

        let response_future = match self.start_requesting_inner(&mut conn, request).await {
            Ok(v) => v,
            Err(e) => {
                response_sender.send(Err(e)).await;
                return;
            }
        };

        conn.num_outstanding_requests += 1;

        // TODO: Eventually directly feed the response_sender to the connection
        // rather than doing chaining here.
        response_sender.send_future(response_future);
    }

    async fn start_requesting_inner(
        &self,
        conn: &mut ConnectionEntry,
        mut request: Request,
    ) -> Result<Pin<Box<dyn Future<Output = Result<Response>> + Send + 'static>>> {
        // TODO: We should just disallow using an authority in requests as it may be
        // inconsistent with which was specified for TLS or for generating a load
        // balanced channel.

        let authority = request
            .head
            .uri
            .authority
            .get_or_insert_with(|| self.shared.endpoint.authority.clone());

        // Normalize the port sent in the Host header to exclude default values.
        let default_port = Some(if conn.is_secure { 443 } else { 80 });
        if default_port == authority.port {
            authority.port = None;
        }

        let actual_scheme = if conn.is_secure { "https" } else { "http" };

        // In general. user's shouldn't provide a scheme or authority in their requests
        // but if they do, ensure that they aren't accidentally getting the wrong level
        // of security.
        if let Some(scheme) = &request.head.uri.scheme {
            if scheme.as_str() != actual_scheme {
                return Err(err_msg(
                    "Mismatch between requested scheme and connection scheme",
                ));
            }
        }

        // TODO: Ensure that the scheme is also validated on the server.

        match &conn.instance {
            ConnectionInstance::V2(conn) => {
                request.head.uri.scheme = Some(AsciiString::from(actual_scheme).unwrap());

                let response = conn.enqueue_request(request).await?;
                Ok(Box::pin(response))
            }
            ConnectionInstance::V1(conn) => {
                request.head.uri.scheme = None;

                let res = conn.enqueue_request(request).await?;

                Ok(Box::pin(async move {
                    Ok(match res.await? {
                        v1::ClientConnectionResponse::Regular { response } => response,
                        v1::ClientConnectionResponse::Upgrading { response_head, .. } => {
                            return Err(err_msg("Did not expect an upgrade"));
                        }
                    })
                }))
            }
        }
    }
}

/// Listener for connection events. One of these is connected to every
/// connection managed by the
///
/// NOTE: Functions in this struct MUST NOT lock the State struct of the
/// DirectClient. Normally the DirectClient state is locked prior to the
/// connection state. Because events are called with the connection locked only,
/// locking the other state may lead to deadlock.
struct ConnectionListener {
    connection_id: usize,
    shared: Weak<Shared>,
}

#[async_trait]
impl ConnectionEventListener for ConnectionListener {
    async fn handle_request_completed(&self) {
        if let Some(shared) = self.shared.upgrade() {
            let mut events = shared.received_events.lock().await;
            events
                .connection_events
                .entry(self.connection_id)
                .or_default()
                .num_completed_requests += 1;
            events.notify_all();
        }
    }

    async fn handle_connection_shutdown(&self, details: ConnectionShutdownDetails) {
        if let Some(shared) = self.shared.upgrade() {
            let mut events = shared.received_events.lock().await;

            // TODO: Allow the value of this to change back to false if future connections
            // yield a different outcome (especially if we eventually see an http2
            // connection).
            if details.http1_rejected_persistence {
                events.http1_non_persistent_connections = true;
            }

            let mut entry = events
                .connection_events
                .entry(self.connection_id)
                .or_default();

            entry.shutting_down = true;

            // NOTE: This ignores locally triggered connection shutdowns.
            // TODO: If we believe that a connection is associated with a single backend, we
            // should still backoff on shutdowns (as the server is probably shutting down)
            // (although in this case the next connect attempt should fail anyway).
            if !details.graceful || (!details.local && shared.options.remote_shutdown_is_failure) {
                entry.failed = true;

                let mut overall_state = shared.overall_state.lock().await;
                *overall_state = ClientState::Failure;
                overall_state.notify_all();
                drop(overall_state);
            }

            events.notify_all();
        }
    }
}

// TODO: Add a test case of connecting to an unreachable port to verify that the
// backoff timings are correct.

// TODO: Verify if we have a long running request for which we got the headers
// but the body is still being received that we don't shutdown the connection
// running it.
