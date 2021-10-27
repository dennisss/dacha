use std::collections::HashMap;
use std::convert::TryInto;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use common::async_std::net::TcpStream;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::errors::*;
use common::io::{Readable, Writeable};
use parsing::ascii::AsciiString;

use crate::dns::*;
use crate::header::*;
use crate::method::*;
use crate::request::*;
use crate::response::*;
use crate::uri::*;
use crate::v1;
use crate::v2;

// TODO: ensure that ConnectionRefused or other types of errors that occur
// before we send out the request are all retryable.

// TODO: Need to clearly document which responsibilities are reserved for the
// client.

#[derive(Clone)]
pub struct ClientOptions {
    /// Host optionally with a port to which we should connect.
    pub authority: Authority,

    /// If true, we'll connect using SSL/TLS. By default, we send HTTP2 over
    /// clear text.
    pub secure: bool,

    /// If true, we'll immediately connect using HTTP2 and fail if it is not
    /// supported by the server. By default, we'll start by sending HTTP1
    /// requests until we are confident that the remote server supports
    /// HTTP2.
    pub force_http2: bool, /* TODO: Idle timeout or allow persistent connections */

                           /* TODO: Should have a timeout for establishing a connection. */
}

impl ClientOptions {
    pub fn from_authority<A: TryInto<Authority, Error = Error>>(authority: A) -> Result<Self> {
        Ok(Self {
            authority: authority.try_into()?,
            secure: false,
            force_http2: false,
        })
    }

    pub fn from_uri(uri: &Uri) -> Result<Self> {
        // let uri: Uri = uri.try_into()?;

        let scheme = uri
            .scheme
            .clone()
            .ok_or_else(|| err_msg("Uri missing a scheme"))?
            .as_str()
            .to_ascii_lowercase();

        let secure = match scheme.as_str() {
            "http" => false,
            "https" => true,
            _ => {
                return Err(format_err!("Unsupported scheme: {}", scheme));
            }
        };

        Ok(Self {
            authority: uri
                .authority
                .clone()
                .ok_or_else(|| err_msg("Uri missing an authority"))?,
            secure,
            force_http2: false,
        })
    }

    // TODO: Crate a macro to generate these.
    pub fn set_secure(mut self, value: bool) -> Self {
        self.secure = value;
        self
    }

    pub fn set_force_http2(mut self, value: bool) -> Self {
        self.force_http2 = value;
        self
    }
}

enum ClientConnectionEntry {
    V1(v1::ClientConnection),
    V2(v2::Connection),
}

#[derive(Default)]
struct ClientConnectionPool {
    connections: HashMap<usize, Arc<ClientConnectionEntry>>,
    last_id: usize,
}

/*
TODO: Connections should have an accepting_connections()

    We need information on accepting_connections() in
*/

/// HTTP client connected to a single server.
pub struct Client {
    shared: Arc<ClientShared>,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

struct ClientShared {
    // /// Uri to which we should connection.
    // /// This should only a scheme and authority.
    // base_uri: Uri,
    options: ClientOptions,

    /// TODO: Re-generate this on-demand so that new connections  we start a new
    /// connection as we may want to re-query DNS.
    socket_addr: SocketAddr,

    connection_pool: Mutex<ClientConnectionPool>,
    // A client should have a list of
}

impl Client {
    /// Creates a new HTTP client connecting to the given host/port.
    ///
    /// Arguments:
    /// - authority:
    /// - options: Options for how to start connections
    ///
    /// NOTE: This will not start a connection.
    /// TODO: Instead just take as input an authority string and whether or not
    /// we want it to be secure?
    pub fn create(options: ClientOptions) -> Result<Self> {
        let port = options
            .authority
            .port
            .unwrap_or(if options.secure { 443 } else { 80 });

        // TODO: Whenever we need to create a new connection, consider re-fetching the
        // dns result.
        let ip = match &options.authority.host {
            Host::Name(n) => {
                // TODO: This should become async.
                let addrs = lookup_hostname(n.as_ref())?;
                let mut ip = None;
                // TODO: Prefer ipv6 over ipv4?
                for a in addrs {
                    if a.socket_type == SocketType::Stream {
                        ip = Some(a.address);
                        break;
                    }
                }

                match ip {
                    Some(i) => i,
                    None => {
                        return Err(err_msg("Failed to resolve host to an ip"));
                    }
                }
            }
            Host::IP(ip) => ip.clone(),
        };

        Ok(Client {
            shared: Arc::new(ClientShared {
                // TODO: Check port is in u16 range in the parser
                socket_addr: SocketAddr::new(ip.try_into()?, port as u16),
                options,
                connection_pool: Mutex::new(ClientConnectionPool::default()),
            }),
        })
    }

    // TODO: If we recieve an unterminated body, then we should close the
    // connection right afterwards.

    // TODO: We need to refactor this to re-use existing connections?

    // TODO: request() can be split into two halves,

    /// NOTE: Must be called with a lock on the connection pool
    async fn new_connection(&self, connection_id: usize) -> Result<Arc<ClientConnectionEntry>> {
        let raw_stream = common::async_std::future::timeout(
            std::time::Duration::from_millis(500),
            TcpStream::connect(self.shared.socket_addr),
        )
        .await??;
        raw_stream.set_nodelay(true)?;

        let mut reader: Box<dyn Readable> = Box::new(raw_stream.clone());
        let mut writer: Box<dyn Writeable> = Box::new(raw_stream);

        let mut start_http2 = self.shared.options.force_http2;

        if self.shared.options.secure {
            let mut client_options = crypto::tls::options::ClientOptions::recommended();
            // TODO: Merge with self.options

            if let Host::Name(name) = &self.shared.options.authority.host {
                client_options.hostname = name.clone();
            }
            client_options.alpn_ids.push("h2".into());
            client_options.alpn_ids.push("http/1.1".into());

            // TODO: Require that this by exported to a client level setting.
            client_options.trust_server_certificate = true;

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
                runner,
            ));

            return Ok(Arc::new(ClientConnectionEntry::V2(connection_v2)));
        }

        let conn = v1::ClientConnection::new();

        let conn_runner = task::spawn(Self::connection_runner(
            Arc::downgrade(&self.shared),
            connection_id,
            conn.run(reader, writer),
        ));

        // Attempt to upgrade to HTTP2 over clear text.
        if !self.shared.options.secure && false {
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

        Ok(Arc::new(ClientConnectionEntry::V1(conn)))
    }

    // NOTE: This uses a Weak pointer to ensure that the ClientShared and Connection
    // can be dropped which may lead to the Connection shutting down.
    async fn connection_runner<F: std::future::Future<Output = Result<()>>>(
        client_shared: Weak<ClientShared>,
        connection_id: usize,
        f: F,
    ) {
        if let Err(e) = f.await {
            eprintln!("http::Client Connection failed: {:?}", e);
        }

        if let Some(client_shared) = client_shared.upgrade() {
            let mut connection_pool = client_shared.connection_pool.lock().await;
            connection_pool.connections.remove(&connection_id);
        }
    }

    async fn get_connection(&self) -> Result<Arc<ClientConnectionEntry>> {
        let mut pool = self.shared.connection_pool.lock().await;

        let first_connection = pool.connections.values().next();
        if let Some(connection) = first_connection {
            return Ok(connection.clone());
        }

        let connection_id = pool.last_id + 1;
        pool.last_id = connection_id;

        let connection = self.new_connection(connection_id).await?;

        pool.connections.insert(connection_id, connection.clone());
        Ok(connection)
    }

    // Given request, if not connected, connect
    // Write request to stream
    // Read response
    // - TODO: Response may be available before the request is sent (in the case of
    //   bodies)
    // If not using a content length, then we should close the connection
    pub async fn request(&self, mut request: Request) -> Result<Response> {
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

        if !request.head.uri.authority.is_some() {
            request.head.uri.authority = Some(self.shared.options.authority.clone());
        }

        let conn_entry = self.get_connection().await?;

        match conn_entry.as_ref() {
            ClientConnectionEntry::V2(conn) => {
                // TODO: Make this less hard coded.
                request.head.uri.scheme = Some(AsciiString::from("https").unwrap());

                let response = conn.request(request).await?;
                Ok(response)
            }
            ClientConnectionEntry::V1(conn) => {
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

// Other challenges: If we are going to have an HTTP 1.1 connection pool, then
// we could re-use the

// If there is an upgrade pending, then we can't

/*
TODO:
A server MUST NOT switch protocols unless the received message semantics can be honored by the new protocol

*/

/*
    Suppose we get a
*/

// Upgraded: Will kill

/*
    Key details about an upgrade request:
    - We shouldn't send any more requests on a connection which is in the process of being upgraded.
        - This implies that we should know if we're upgrading

*/
