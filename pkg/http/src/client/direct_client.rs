use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use common::async_std::channel;
use common::async_std::net::TcpStream;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::condvar::Condvar;
use common::errors::*;
use common::io::{Readable, Writeable};
use parsing::ascii::AsciiString;

use crate::backoff::*;
use crate::client::client_interface::*;
use crate::client::resolver::ResolvedEndpoint;
use crate::header::*;
use crate::method::*;
use crate::request::*;
use crate::response::Response;
use crate::uri::*;
use crate::{v1, v2};

/*
General events to listen for:
- Connection died (either gracefully or not)
- Connection established
- based on that, we can tell if we need to generate new connections.
- Until we successfully connect one thread,
*/

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
    pub tls: Option<crypto::tls::options::ClientOptions>,

    /// If true, we'll immediately connect using HTTP2 and fail if it is not
    /// supported by the server. By default, we'll start by sending HTTP1
    /// requests until we are confident that the remote server supports
    /// HTTP2.
    pub force_http2: bool,

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
    /// TODO: Implement this.
    pub idle_timeout: Duration,
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

    state: Condvar<ClientState>,
    connection_pool: Mutex<ConnectionPool>,
}

#[derive(Default)]
struct ConnectionPool {
    connections: HashMap<usize, Arc<ConnectionEntry>>,
}

enum ConnectionEntry {
    V1(v1::ClientConnection),
    V2(v2::Connection),
}

enum ConnectionEvent {
    Connected(usize),
    Failed(usize),
}

impl DirectClient {
    pub fn new(endpoint: ResolvedEndpoint, options: DirectClientOptions) -> Self {
        Self {
            shared: Arc::new(Shared {
                endpoint,
                options,
                state: Condvar::new(ClientState::NotConnected),
                connection_pool: Mutex::new(ConnectionPool::default()),
            }),
        }
    }

    pub async fn run(self) {
        let (sender, receiver) = channel::bounded(1);

        let mut backoff = ExponentialBackoff::new(self.shared.options.connection_backoff.clone());
        let mut last_id = 0;

        loop {
            // Check if we need to start new connections.
            // Currently we just focus on retaining one healthy connection.
            while self.shared.connection_pool.lock().await.connections.len() != 1 {
                // Mark state as connecting (if we didn't fail the last attempt).
                {
                    let mut state = self.shared.state.lock().await;
                    if *state != ClientState::Failure {
                        *state = ClientState::Connecting;
                    }

                    state.notify_all();
                }

                match backoff.start_attempt() {
                    ExponentialBackoffResult::Start => {}
                    ExponentialBackoffResult::StartAfter(wait_time) => task::sleep(wait_time).await,
                    ExponentialBackoffResult::Stop => {
                        eprintln!("DirectClient ran out of connection attempts");
                    }
                }

                let connection_id = last_id + 1;
                last_id = connection_id;

                let successful = match self.new_connection(connection_id, sender.clone()).await {
                    Ok(v) => {
                        self.shared
                            .connection_pool
                            .lock()
                            .await
                            .connections
                            .insert(connection_id, v);
                        true
                    }
                    Err(e) => {
                        eprintln!("Failure while connecting {:?}: {}", self.shared.endpoint, e);
                        false
                    }
                };

                // TODO: If there is a failure very soon after the connection starts, we should
                // increase the timeout time.

                backoff.end_attempt(successful);

                {
                    let mut state = self.shared.state.lock().await;
                    *state = if successful {
                        ClientState::Connected
                    } else {
                        ClientState::Failure
                    };
                    state.notify_all();
                }
            }

            // All the work we can do so far is done. Wait for something to
            // happen.
            let _ = receiver.recv().await;
        }
    }

    /// NOTE: Must be called with a lock on the connection pool to ensure that
    /// no one else is also making one at the same time.
    async fn new_connection(
        &self,
        connection_id: usize,
        closed_notifier: channel::Sender<()>,
    ) -> Result<Arc<ConnectionEntry>> {
        // Ways in which this can fail:
        // - Timeout: Unable to reach the destination ip.
        // - io::ErrorKind::ConnectionRefused: REached the server but it's not serving
        //   on the given port.
        //
        // TODO: Push the timeout to wrap more of the connection process (like the TLS
        // handshake).
        let raw_stream = common::async_std::future::timeout(
            self.shared.options.connect_timeout.clone(),
            TcpStream::connect(self.shared.endpoint.address),
        )
        .await??;
        raw_stream.set_nodelay(true)?;

        let mut reader: Box<dyn Readable> = Box::new(raw_stream.clone());
        let mut writer: Box<dyn Writeable> = Box::new(raw_stream);

        let mut start_http2 = self.shared.options.force_http2;

        if let Some(mut client_options) = self.shared.options.tls.clone() {
            if let Host::Name(name) = &self.shared.endpoint.authority.host {
                client_options.hostname = name.clone();
            }
            client_options.alpn_ids.push("h2".into());
            client_options.alpn_ids.push("http/1.1".into());

            // TODO: Require that this by exported to a client level setting.
            // client_options.trust_server_certificate = true;

            let mut tls_client = crypto::tls::client::Client::new();

            let tls_stream = tls_client.connect(reader, writer, &client_options).await?;

            reader = Box::new(tls_stream.reader);
            writer = Box::new(tls_stream.writer);

            if let Some(protocol) = tls_stream.handshake_summary.selected_alpn_protocol {
                if protocol.as_ref() == b"h2" {
                    start_http2 = true;
                    println!("NEGOTIATED HTTP2 OVER TLS");
                }
            }
        }

        if start_http2 {
            let connection_options = v2::ConnectionOptions::default();

            let connection_v2 = v2::Connection::new(connection_options, None);

            let initial_state = v2::ConnectionInitialState::raw();

            let runner = connection_v2.run(initial_state, reader, writer);
            task::spawn(Self::connection_runner(
                Arc::downgrade(&self.shared),
                connection_id,
                closed_notifier,
                runner,
            ));

            return Ok(Arc::new(ConnectionEntry::V2(connection_v2)));
        }

        let conn = v1::ClientConnection::new();

        // TODO: Take care of this return value.
        let conn_runner = task::spawn(Self::connection_runner(
            Arc::downgrade(&self.shared),
            connection_id,
            closed_notifier,
            conn.run(reader, writer),
        ));

        // Attempt to upgrade to HTTP2 over clear text.
        if !self.shared.options.tls.is_some() && false {
            let local_settings = crate::v2::SettingsContainer::default();

            let mut connection_options = vec![];
            connection_options.push(crate::headers::connection::ConnectionOption::Unknown(
                parsing::ascii::AsciiString::from("Upgrade").unwrap(),
            ));

            // TODO: Copy the host and uri from the request.
            let mut upgrade_request = RequestBuilder::new()
                .method(Method::GET)
                // .uri("http://www.google.com/")
                // .header("Host", "www.google.com")
                .header(CONNECTION, "Upgrade, HTTP2-Settings")
                .header("Upgrade", "h2c")
                .build()
                .unwrap();

            local_settings
                .append_to_request(&mut upgrade_request.head.headers, &mut connection_options);
            // TODO: Serialize the connection options vector into the header.

            // TODO: Explicitly enqueue the requests. If the connection dies but we never
            // started sending the reuqest, then we can immediately re-try it.
            let res = conn.request(upgrade_request).await?;

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

        Ok(Arc::new(ConnectionEntry::V1(conn)))
    }

    // NOTE: This uses a Weak pointer to ensure that the ClientShared and Connection
    // can be dropped which may lead to the Connection shutting down.
    async fn connection_runner<F: std::future::Future<Output = Result<()>>>(
        client_shared: Weak<Shared>,
        connection_id: usize,
        closed_notifier: channel::Sender<()>,
        f: F,
    ) {
        if let Err(e) = f.await {
            eprintln!("http::Client Connection failed: {:?}", e);
        }

        if let Some(client_shared) = client_shared.upgrade() {
            let mut connection_pool = client_shared.connection_pool.lock().await;
            connection_pool.connections.remove(&connection_id);

            {
                let mut state = client_shared.state.lock().await;
                *state = ClientState::Failure;
                state.notify_all();
            }

            let _ = closed_notifier.send(()).await;
        }
    }

    async fn get_connection(&self) -> Result<Arc<ConnectionEntry>> {
        loop {
            let state = self.shared.state.lock().await;
            if *state != ClientState::Connected {
                state.wait(()).await;
                continue;
            }

            let pool = self.shared.connection_pool.lock().await;

            // TODO: it would be an error if there are no connections present.
            let first_connection = pool.connections.values().next();
            if let Some(connection) = first_connection {
                return Ok(connection.clone());
            }
        }
    }
}

#[async_trait]
impl ClientInterface for DirectClient {
    // Given request, if not connected, connect
    // Write request to stream
    // Read response
    // - TODO: Response may be available before the request is sent (in the case of
    //   bodies)
    // If not using a content length, then we should close the connection
    async fn request(&self, mut request: Request) -> Result<Response> {
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

        // TODO: Only pop this if we need to perfect an HTTP1 request (in HTTP2 we can
        // forward a lot of stuff).
        if let Some(scheme) = request.head.uri.scheme.take() {
            // TODO: Verify if 'http(s)' as others aren't supported by this
            // client.
        } else {
            // return Err(err_msg("Missing scheme in URI"));
        }

        // TODO: Explicitly always set the scheme based on the

        // TODO: We should just disallow using an authority in requests as it goes
        // against TLS and load balancing assumptions.

        //
        if !request.head.uri.authority.is_some() {
            request.head.uri.authority = Some(self.shared.endpoint.authority.clone());
            // TODO: Remove default ports (80 and 443).
        }

        let conn_entry = self.get_connection().await?;

        match conn_entry.as_ref() {
            ConnectionEntry::V2(conn) => {
                // TODO: Make this less hard coded.
                request.head.uri.scheme = Some(AsciiString::from("https").unwrap());

                let response = conn.request(request).await?;
                Ok(response)
            }
            ConnectionEntry::V1(conn) => {
                let res = conn.request(request).await?;

                let res = match res {
                    v1::ClientConnectionResponse::Regular { response } => response,
                    v1::ClientConnectionResponse::Upgrading { response_head, .. } => {
                        return Err(err_msg("Did not expect an upgrade"));
                    }
                };

                Ok(res)
            }
        }
    }
}
