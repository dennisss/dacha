use std::sync::Arc;
use std::convert::{TryFrom, TryInto};
use std::net::SocketAddr;

use common::{async_std::net::TcpStream};
use common::async_std::prelude::*;
use common::errors::*;
use common::borrowed::Borrowed;
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::io::{Readable, Writeable};
use common::async_std::task;
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::uri_syntax::serialize_authority;
use crate::dns::*;
use crate::header::*;
use crate::header_syntax::*;
use crate::message::*;
use crate::message_syntax::*;
use crate::reader::*;
use crate::spec::*;
use crate::status_code::*;
use crate::uri::*;
use crate::response::*;
use crate::request::*;
use crate::method::*;
use crate::message_body::{encode_request_body_v1, decode_response_body_v1};

// TODO: Need to clearly document which responsibilities are reserved for the client.

#[derive(Clone)]
pub struct ClientOptions {
    /// Host optionally with a port to which we should connect.
    pub authority: Authority,

    /// If true, we'll connect using SSL/TLS. By default, we send HTTP2 over clear text.
    pub secure: bool,

    /// If true, we'll immediately connect using HTTP2 and fail if it is not supported by the
    /// server. By default, we'll start by sending HTTP1 requests until we are confident that
    /// the remote server supports HTTP2.
    pub force_http2: bool
}

impl ClientOptions {
    pub fn from_authority<A: TryInto<Authority, Error=Error>>(authority: A) -> Result<Self> {
        Ok(Self {
            authority: authority.try_into()?,
            secure: false,
            force_http2: false
        })
    }

    pub fn from_uri(uri: &Uri) -> Result<Self> {
        // let uri: Uri = uri.try_into()?;

        let scheme = uri.scheme.clone().ok_or_else(|| err_msg("Uri missing a scheme"))?
            .as_str().to_ascii_lowercase();

        let secure = match scheme.as_str() {
            "http" => false,
            "https" => true,
            _ => { return Err(format_err!("Unsupported scheme: {}", scheme)); }
        };

        Ok(Self {
            authority: uri.authority.clone().ok_or_else(|| err_msg("Uri missing an authority"))?,
            secure,
            force_http2: false
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

/// HTTP client connected to a single server.
pub struct Client {
    // /// Uri to which we should connection.
    // /// This should only a scheme and authority.
    // base_uri: Uri,

    options: ClientOptions,

    /// TODO: Re-generate this on-demand so that new connections  we start a new connection as we may want to re-query DNS.
    socket_addr: SocketAddr,

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
    /// TODO: Instead just take as input an authority string and whether or not we want it to be secure?
    pub fn create(options: ClientOptions) -> Result<Self> {
        let port = options.authority.port.unwrap_or(if options.secure { 443 } else { 80 });

        // TODO: Whenever we need to create a new connection, consider 
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
            // TODO: Check port is in u16 range in the parser
            socket_addr: SocketAddr::new(ip.try_into()?, port as u16),
            options
        })
    }

    // TODO: If we recieve an unterminated body, then we should close the
    // connection right afterwards.

    // Given request, if not connected, connect
    // Write request to stream
    // Read response
    // - TODO: Response may be available before the request is sent (in the case of
    //   bodies)
    // If not using a content length, then we should close the connection
    pub async fn request(&self, mut request: Request) -> Result<Response> {
        // TODO: We should allow the Connection header, but we shouldn't allow any options
        // which are used internally (keep-alive and close)
        for header in &request.head.headers.raw_headers {
            if header.is_transport_level() {
                return Err(format_err!("Request contains reserved header: {}", header.name.as_str()));
            }
        }

        // TODO: Only pop this if we need to perfect an HTTP1 request (in HTTP2 we can forward a lot of stuff).
        if let Some(scheme) = request.head.uri.scheme.take() {
            // TODO: Verify if 'http(s)' as others aren't supported by this client.  
        } else {
            // return Err(err_msg("Missing scheme in URI"));
        }

        if !request.head.uri.authority.is_some() {
            request.head.uri.authority = Some(self.options.authority.clone());
        }

        // TODO: For an empty body, the client doesn't need to send any special headers.

        // TODO: Use timeout?
        let raw_stream = TcpStream::connect(self.socket_addr).await?;
        raw_stream.set_nodelay(true)?;

        let mut reader: Box<dyn Readable> = Box::new(raw_stream.clone());
        let mut writer: Box<dyn Writeable> = Box::new(raw_stream);

        let mut start_http2 = self.options.force_http2;

        if self.options.secure {
            let mut client_options = crypto::tls::options::ClientOptions::recommended();
            // TODO: 

            if let Host::Name(name) = &self.options.authority.host {
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
            let connection_options = crate::v2::ConnectionOptions::default();

            let connection_v2 = crate::v2::Connection::new(connection_options, None);

            let initial_state = crate::v2::ConnectionInitialState::raw();

            let conn_runner = task::spawn(
                connection_v2.run(initial_state, reader, writer));

            request.head.uri.scheme = Some(AsciiString::from("https").unwrap());

            let response = connection_v2.request(request).await?;
            // TODO: Shut down the connection and join the conn_runner.

            connection_v2.shutdown(true).await?;

            conn_runner.await?;

            return Ok(response);
        }



        let conn = ClientConnection::new();

        let conn_runner = task::spawn(
            conn.shared.clone().run(reader, writer));

        // Attempt to upgrade to HTTP2 over clear text.
        if !self.options.secure && false {
            let local_settings = crate::v2::SettingsContainer::default();

            let mut connection_options = vec![];
            connection_options.push(crate::headers::connection::ConnectionOption::Unknown(
                parsing::ascii::AsciiString::from("Upgrade").unwrap()));

            // TODO: Copy the host and uri from the request.
            let mut upgrade_request = RequestBuilder::new()
                .method(Method::GET)
                // .uri("http://www.google.com/")
                // .header("Host", "www.google.com")
                .header(CONNECTION, "Upgrade, HTTP2-Settings")
                .header("Upgrade", "h2c")
                .build()
                .unwrap();

            local_settings.append_to_request(&mut upgrade_request.head.headers, &mut connection_options);
            // TODO: Serialize the connection options vector into the header.

            let (sender, receiver) = channel::bounded(1);

            conn.shared.connection_event_channel.0.send(ClientConnectionEvent::Request {
                request: upgrade_request,
                upgrading: false,
                response_handler: sender
            }).await.map_err(|_| err_msg("Connection hung up"))?;

            let res = receiver.recv().await??;

            let res = match res {
                ClientResponse::Regular { response } => {
                    println!("{:?}", response.head);
                    println!("DID NOT UPGRADE")
                },
                ClientResponse::Upgrading { response_head, .. } => {
                    return Err(err_msg("UPGRADING"));
                }
            };
        }


        let (sender, receiver) = channel::bounded(1);

        conn.shared.connection_event_channel.0.send(ClientConnectionEvent::Request {
            request,
            upgrading: false,
            response_handler: sender
        }).await.map_err(|_| err_msg("Connection hung up"))?;

        let res = receiver.recv().await??;
        let res = match res {
            ClientResponse::Regular { response } => response,
            ClientResponse::Upgrading { response_head, .. } => {
                return Err(err_msg("Did not expect an upgrade"));
            }
        };

        if let Some(r) = conn_runner.cancel().await {
            r?;
        }

        

        Ok(res)

    }

    // pub async fn request_upgrade()
}


enum ClientConnectionEvent {
    Request {
        request: Request,
        upgrading: bool,
        response_handler: channel::Sender<Result<ClientResponse>>
    }
}

// Other challenges: If we are going to have an HTTP 1.1 connection pool, then we could re-use the 

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

/// Stores the 
enum ClientResponse {
    Regular {
        response: Response
    },
    Upgrading {
        response_head: ResponseHead, 
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>
    },
    
}


struct ClientConnection {
    shared: Arc<ClientConnectionShared>,
}

impl ClientConnection {
    fn new() -> Self {
        ClientConnection {
            shared: Arc::new(ClientConnectionShared {
                connection_event_channel: channel::unbounded(),
                state: Mutex::new(ClientConnectionState {
                    running: false,
                    pending_upgrade: false
                })
            })
        }
    }
}

struct ClientConnectionShared {
    connection_event_channel: (channel::Sender<ClientConnectionEvent>, channel::Receiver<ClientConnectionEvent>),
    state: Mutex<ClientConnectionState>
}

struct ClientConnectionState {
    running: bool,


    /*
        State can be:
        - Running
        - PendingUpgrade
        - Upgraded
        - ErroredOut
    */

    // Either<Response, UpgradeResponse>

    /// If true, then we sent a request on this connection to try to upgrade 
    pending_upgrade: bool,
}


impl ClientConnectionShared {
    /*
        Creating a new client connection:
        - If we know that the server supports HTTP2 (or we force it),
            - Run an internal HTTP2 connection (pass all burden onto that)
        - Else
            - Run an 'OPTIONS *' request in order to attempt an upgrade to HTTP 2 (or maybe get some Alt-Svcs)
            - If we upgraded, Do it!!!

        - Future optimization:
            - If we are sending a request as soon as the client is created, we can use that as the upgrade request
                instead of the 'OPTION *' to avoid a 
    */

    async fn run(self: Arc<Self>, reader: Box<dyn Readable>, writer: Box<dyn Writeable>) -> Result<()> {
        let r = self.run_inner(reader, writer).await;
        println!("ClientConnection: {:?}", r);
        r
    }

    async fn run_inner(self: Arc<Self>, reader: Box<dyn Readable>, mut writer: Box<dyn Writeable>) -> Result<()> {

        let mut reader = PatternReader::new(reader, MESSAGE_HEAD_BUFFER_OPTIONS);

        loop {
            let e = self.connection_event_channel.1.recv().await?;

            match e {
                ClientConnectionEvent::Request { mut request, upgrading, response_handler } => {
                    // TODO: When using the 'Host' header, we can't provie the userinfo
                    if let Some(authority) = request.head.uri.authority.take() {
                        let mut value = vec![];
                        serialize_authority(&authority, &mut value)?;

                        // TODO: Ensure that this is the first header sent.
                        request.head.headers.raw_headers.push(Header {
                            name: AsciiString::from(HOST).unwrap(),
                            value: OpaqueString::from(value)
                        });
                    } else {
                        return Err(err_msg("Missing authority in URI"));
                    }
                
                    // This is mainly needed to allow talking to HTTP 1.0 servers (in 1.1 it is
                    // the default).
                    // TODO: USe the append_connection_header() method.
                    // TODO: It may have "Upgrade" so we need to be careful to concatenate values here.
                    request.head.headers.raw_headers.push(Header {
                        name: AsciiString::from(CONNECTION).unwrap(),
                        value: "keep-alive".into()
                    });

                    let mut body = encode_request_body_v1(&mut request.head, request.body);

                    let mut out = vec![];
                    // TODO: If this fails, we should notify the local requster rather than
                    // bailing out on the entire connection.
                    request.head.serialize(&mut out)?;
                    writer.write_all(&out).await?;
                    write_body(body.as_mut(), writer.as_mut()).await?;

                    let head = match read_http_message(&mut reader).await? {
                        HttpStreamEvent::MessageHead(h) => h,
                        // TODO: Handle other bad cases such as too large headers.
                        _ => {
                            return Err(err_msg("Connection closed without a complete response"));
                        }
                    };

                    let body_start_idx = head.len();

                    //		println!("{:?}", String::from_utf8(head.to_vec()).unwrap());

                    let msg = match parse_http_message_head(head) {
                        Ok((msg, rest)) => {
                            assert_eq!(rest.len(), 0);
                            msg
                        }
                        Err(e) => {
                            // TODO: Consolidate these lines.
                            println!("Failed to parse message\n{}", e);
                            return Err(err_msg("Invalid message received"));
                        }
                    };

                    let start_line = msg.start_line;
                    let headers = msg.headers;

                    // Verify that we got a Request style message
                    let status_line = match start_line {
                        StartLine::Request(r) => {
                            return Err(err_msg("Received a request?"));
                        }
                        StartLine::Response(r) => r,
                    };

                    let status_code = StatusCode::from_u16(status_line.status_code)
                        .ok_or(Error::from(err_msg("Invalid status code")))?;

                    let head = ResponseHead {
                        version: status_line.version,
                        // TODO: Print the code in the error case
                        status_code,
                        reason: status_line.reason,
                        headers,
                    };


                    let persist_connection = crate::headers::connection::can_connection_persist(
                        &head.version, &head.headers)?;

                    if head.status_code == SWITCHING_PROTOCOLS {
                        let _ = response_handler.try_send(Ok(ClientResponse::Upgrading {
                            response_head: head,
                            reader: Box::new(reader),
                            writer
                        }));
                        return Ok(());
                    }

                    let (body, reader_returner) = decode_response_body_v1(
                        request.head.method, &head, reader)?;


                    let _ = response_handler.try_send(Ok(ClientResponse::Regular {
                        response: Response { head, body }
                    }));

                    // With a well framed response body, we can perist the connection.
                    if persist_connection {
                        if let Some(returner) = reader_returner {
                            reader = returner.wait().await?;
                            continue;
                        }
                    }

                    // Connection can no longer persist.
                    break;
                }
            }
        }


        Ok(())
    }

}

