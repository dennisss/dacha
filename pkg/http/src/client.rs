use std::sync::Arc;
use std::convert::TryInto;
use std::net::SocketAddr;

use common::{async_std::net::TcpStream};
use common::async_std::prelude::*;
use common::errors::*;
use common::borrowed::Borrowed;
use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::io::{Readable, Writeable};
use common::async_std::task;

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

// TODO: Need to clearly document which responsibilities are reserved for the client.

/// HTTP client connected to a single server.
pub struct Client {
    socket_addr: SocketAddr,

    // A client should have a list of 
}

impl Client {
    /// Creates a new client connecting to the given host/protocol.
    /// NOTE: This will not start a connection.
    pub fn create(uri: &str) -> Result<Client> {
        // TODO: Implement some other form of parser function that doesn't
        // accept anything but the scheme, authority
        let u = uri.parse::<Uri>()?;
        // NOTE: u.path may be '/'
        if !u.path.as_ref().is_empty() || u.query.is_some() || u.fragment.is_some() {
            return Err(err_msg("Can't create a client with a uri path"));
        }

        let scheme = u
            .scheme
            .map(|s| s.to_string())
            .unwrap_or("http".into())
            .to_ascii_lowercase();

        let (default_port, is_secure) = match scheme.as_str() {
            "http" => (80, false),
            "https" => (443, true),
            _ => {
                // TODO: Create an err! macro
                return Err(format_err!("Unsupported scheme {}", scheme));
            }
        };

        if is_secure {
            return Err(err_msg("TLS/SSL currently not supported"));
        }

        let authority = u
            .authority
            .ok_or(err_msg("No authority/hostname specified"))?;

        // TODO: Definately need a more specific type of Uri to ensure that we
        // don't miss any fields.
        if authority.user.is_some() {
            return Err(err_msg("Users not supported"));
        }

        let port = authority.port.unwrap_or(default_port);

        let ip = match authority.host {
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
            Host::IP(ip) => ip,
        };

        Ok(Client {
            // TODO: Check port is in u16 range in the parser
            socket_addr: SocketAddr::new(ip.try_into()?, port as u16),
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
        if
        request.head.headers.has(CONNECTION) ||
        request.head.headers.has(CONTENT_LENGTH) ||
        request.head.headers.has(HOST) ||
        request.head.headers.has(KEEP_ALIVE) || request.head.headers.has(TRANSFER_ENCODING) {
            return Err(err_msg("Given reserved header"));
        }

        if let Some(scheme) = request.head.uri.scheme.take() {
            // TODO: Verify if 'http(s)' as others aren't supported by this client.  
        } else {
            return Err(err_msg("Missing scheme in URI"));
        }

        // TODO: For an empty body, the client doesn't need to send any special headers.

        // TODO: Use timeout?
        let stream = TcpStream::connect(self.socket_addr).await?;
        stream.set_nodelay(true)?;

        let conn = ClientConnection::new();

        let conn_runner = task::spawn(conn.shared.clone().run(
            Box::new(stream.clone()), Box::new(stream)));

        {
            let mut local_settings = crate::v2::SettingsContainer::default();

            let mut connection_options = vec![];
            connection_options.push(crate::headers::connection::ConnectionOption::Unknown(
                parsing::ascii::AsciiString::from("Upgrade").unwrap()));

            // TODO: Copy the host from the request.
            let mut upgrade_request = RequestBuilder::new()
                .method(Method::GET)
                .uri("http://www.google.com/")
                // .header("Host", "www.google.com")
                .header("Connection", "Upgrade, HTTP2-Settings")
                .header("Upgrade", "h2c")
                .body(crate::body::EmptyBody())
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

        // How to block it 

        loop {
            let e = self.connection_event_channel.1.recv().await?;

            match e {
                ClientConnectionEvent::Request { mut request, upgrading, response_handler } => {
                    // TODO: Set 'Connection: keep-alive' to support talking to legacy (1.0 servers)

                    // TODO: When using the 'Host' header, we can't provie the userinfo
                    if let Some(authority) = request.head.uri.authority.take() {
                        let mut value = vec![];
                        serialize_authority(&authority, &mut value)?;

                        // TODO: Ensure that this is the first header sent.
                        request.head.headers.raw_headers.push(Header {
                            name: parsing::ascii::AsciiString::from(HOST).unwrap(),
                            value: parsing::opaque::OpaqueString::from(value)
                        });
                    } else {
                        return Err(err_msg("Missing authority in URI"));
                    }

                    let mut out = vec![];
                    request.head.serialize(&mut out)?;
                    writer.write_all(&out).await?;
                    write_body(request.body.as_mut(), writer.as_mut()).await?;

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

                    // TODO: If this is a HEAD request, do not receive any body

                    // If chunked encoding is used, then it msut b

                    // A sender MUST NOT send a Content-Length header field in any message
                    //    that contains a Transfer-Encoding header field.

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

                    let (body, reader_returner) = crate::message_body::create_client_response_body(
                        &request, &head, reader)?;


                    let _ = response_handler.try_send(Ok(ClientResponse::Regular {
                        response: Response { head, body }
                    }));

                    println!("WAITING FOR FULLY READ");

                    // With a well framed response body, we can perist the connection.
                    if persist_connection {
                        if let Some(returner) = reader_returner {
                            reader = returner.wait().await?;
                            continue;
                        }
                    }

                    println!("ON TO NEXT REQUEST");

                    // Connection can no longer persist.
                    break;
                }
            }
        }


        Ok(())
    }

}

