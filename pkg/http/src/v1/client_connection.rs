use std::sync::Arc;

use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::errors::*;
use common::io::{Readable, Writeable};
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::header::{Header, CONNECTION, HOST};
use crate::message::{read_http_message, HttpStreamEvent, StartLine, MESSAGE_HEAD_BUFFER_OPTIONS};
use crate::message_body::{decode_response_body_v1, encode_request_body_v1};
use crate::message_syntax::parse_http_message_head;
use crate::reader::PatternReader;
use crate::request::Request;
use crate::response::{Response, ResponseHead};
use crate::spec::write_body;
use crate::status_code::{StatusCode, SWITCHING_PROTOCOLS};
use crate::uri_syntax::serialize_authority;

// TODO: Important to distinguish between channel failure and hitting something
// like an enqueued limit.

// TODO: After we send an upgrade request on a connection, we shouldn't allow
// making additional requests.

/// Wrapper around a request initiated by a client on a ClientConnection.
///
/// Used internally to coordinate callbacks between threads.
struct ClientConnectionRequest {
    request: Request,
    upgrading: bool,
    response_handler: channel::Sender<Result<ClientConnectionResponse>>,
}

/// Result returned to a client after making a single request on the conneciton.
pub enum ClientConnectionResponse {
    Regular {
        response: Response,
    },
    Upgrading {
        response_head: ResponseHead,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
    },
}

///
///
/// TODO: On drop, mark the runner as closing.
pub struct ClientConnection {
    shared: Arc<ClientConnectionShared>,
}

impl ClientConnection {
    /// Creates a new connection instance.
    /// In order for the connection to be actually be useful, the caller should
    /// follow up by running ClientConection::run() on a separate thread to
    /// handle background management of the connection.
    pub fn new() -> Self {
        ClientConnection {
            shared: Arc::new(ClientConnectionShared {
                connection_event_channel: channel::unbounded(),
                state: Mutex::new(ClientConnectionState {
                    running: false,
                    pending_upgrade: false,
                }),
            }),
        }
    }

    pub fn run(
        &self,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
    ) -> impl std::future::Future<Output = Result<()>> {
        self.shared.clone().run(reader, writer)
    }

    pub async fn request(&self, request: Request) -> Result<ClientConnectionResponse> {
        let (sender, receiver) = channel::bounded(1);

        // TODO: Lock the state and verify that the connection isn't already

        // TODO: Handle this error.
        self.shared
            .connection_event_channel
            .0
            .send(ClientConnectionRequest {
                request,
                upgrading: false,
                response_handler: sender,
            })
            .await
            .map_err(|_| err_msg("Connection hung up"))?;

        receiver.recv().await?
    }
}

struct ClientConnectionShared {
    connection_event_channel: (
        channel::Sender<ClientConnectionRequest>,
        channel::Receiver<ClientConnectionRequest>,
    ),
    state: Mutex<ClientConnectionState>,
}

struct ClientConnectionState {
    // TODO: Prevent running twice.
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

    async fn run(
        self: Arc<Self>,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
    ) -> Result<()> {
        let r = self.run_inner(reader, writer).await;
        println!("HTTPv1 client connection closing with: {:?}", r);

        // TODO: Notify all requests

        // while let Ok()

        r
    }

    async fn run_inner(
        self: Arc<Self>,
        reader: Box<dyn Readable>,
        mut writer: Box<dyn Writeable>,
    ) -> Result<()> {
        let mut reader = PatternReader::new(reader, MESSAGE_HEAD_BUFFER_OPTIONS);

        loop {
            let ClientConnectionRequest {
                mut request,
                upgrading,
                response_handler,
            } = self.connection_event_channel.1.recv().await?;

            // TODO: When using the 'Host' header, we can't provie the userinfo
            if let Some(authority) = request.head.uri.authority.take() {
                let mut value = vec![];
                serialize_authority(&authority, &mut value)?;

                // TODO: Ensure that this is the first header sent.
                request.head.headers.raw_headers.push(Header {
                    name: AsciiString::from(HOST).unwrap(),
                    value: OpaqueString::from(value),
                });
            } else {
                return Err(err_msg("Missing authority in URI"));
            }

            // This is mainly needed to allow talking to HTTP 1.0 servers (in 1.1 it is
            // the default).
            // TODO: USe the append_connection_header() method.
            // TODO: It may have "Upgrade" so we need to be careful to concatenate values
            // here.
            request.head.headers.raw_headers.push(Header {
                name: AsciiString::from(CONNECTION).unwrap(),
                value: "keep-alive".into(),
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

            let persist_connection =
                crate::headers::connection::can_connection_persist(&head.version, &head.headers)?;

            if head.status_code == SWITCHING_PROTOCOLS {
                let _ = response_handler.try_send(Ok(ClientConnectionResponse::Upgrading {
                    response_head: head,
                    reader: Box::new(reader),
                    writer,
                }));
                return Ok(());
            }

            let (body, reader_returner) =
                decode_response_body_v1(request.head.method, &head, reader)?;

            let _ = response_handler.try_send(Ok(ClientConnectionResponse::Regular {
                response: Response { head, body },
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

        Ok(())
    }
}
