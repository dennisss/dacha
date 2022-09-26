use std::future::Future;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use common::async_std::channel;
use common::async_std::sync::Mutex;
use common::errors::*;
use common::io::{Readable, Writeable};
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::connection_event_listener::{ConnectionEventListener, ConnectionShutdownDetails};
use crate::header::{Header, CONNECTION, HOST};
use crate::message::{read_http_message, HttpStreamEvent, StartLine, MESSAGE_HEAD_BUFFER_OPTIONS};
use crate::message_body::{
    decode_response_body_v1, encode_request_body_v1, BodyReturnHandler, ReturnedBody,
};
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
        let (event_sender, event_receiver) = channel::unbounded();

        ClientConnection {
            shared: Arc::new(ClientConnectionShared {
                event_sender,
                return_channel: channel::unbounded(),
                state: Mutex::new(ClientConnectionState {
                    event_receiver: Some(event_receiver),
                    event_listener: None,
                }),
            }),
        }
    }

    pub async fn set_event_listener(
        &self,
        event_listener: Box<dyn ConnectionEventListener>,
    ) -> Result<()> {
        let mut state = self.shared.state.lock().await;
        if state.event_listener.is_some() {
            return Err(err_msg(
                "Can not only set listeners before start of main connection thread",
            ));
        }

        state.event_listener = Some(event_listener);
        Ok(())
    }

    /// Requests that the connection is closed soon.
    /// Currently only a graceful shutdown that occurs after the last request is
    /// done is supported.
    pub fn shutdown(&self) {
        self.shared.event_sender.close();
    }

    pub fn accepting_requests(&self) -> bool {
        !self.shared.event_sender.is_closed()
    }

    pub fn run(
        &self,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
    ) -> impl std::future::Future<Output = Result<()>> {
        self.shared.clone().run(reader, writer)
    }

    /// Makes a request using this connection.
    ///
    /// - This function is quick and returns as soon as the request is
    ///   successfully enqueued.
    /// - The returned future will actually wait for the completion of the
    ///   request.
    pub async fn enqueue_request(
        &self,
        request: Request,
    ) -> Result<impl Future<Output = Result<ClientConnectionResponse>>> {
        // TODO: Convert this to a one-time channel.
        let (sender, receiver) = channel::bounded(1);

        // TODO: Lock the state and verify that the connection isn't already dead.

        // TODO: Handle this error.
        self.shared
            .event_sender
            .send(ClientConnectionRequest {
                request,
                upgrading: false,
                response_handler: sender,
            })
            .await
            .map_err(|_| {
                Error::from(crate::v2::ProtocolErrorV2 {
                    code: crate::proto::v2::ErrorCode::REFUSED_STREAM,
                    local: true,
                    message: "Connection closed before request started.".into(),
                })
            })?;

        Ok(async move {
            receiver.recv().await.map_err(|_| {
                Error::from(crate::v2::ProtocolErrorV2 {
                    code: crate::proto::v2::ErrorCode::INTERNAL_ERROR,
                    local: true,
                    message: "Connection failed without providing an error status.".into(),
                })
            })?
        })
    }
}

struct ClientConnectionShared {
    event_sender: channel::Sender<ClientConnectionRequest>,

    return_channel: (
        channel::Sender<Result<ReturnedBody>>,
        channel::Receiver<Result<ReturnedBody>>,
    ),

    state: Mutex<ClientConnectionState>,
}

struct ClientConnectionState {
    event_receiver: Option<channel::Receiver<ClientConnectionRequest>>,

    /// External event listener.
    event_listener: Option<Box<dyn ConnectionEventListener>>,
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
        let mut event_receiver = {
            self.state
                .lock()
                .await
                .event_receiver
                .take()
                .ok_or_else(|| err_msg("Can not run the connection once"))?
        };

        let external_listener = self.state.lock().await.event_listener.take();

        let mut http1_rejected_persistence = false;

        let r = self
            .run_inner(
                reader,
                writer,
                &mut event_receiver,
                &external_listener,
                &mut http1_rejected_persistence,
            )
            .await;

        if let Some(listener) = external_listener {
            let details = ConnectionShutdownDetails {
                /// TODO: Also support checking to see if the TCP connection
                /// gracefully shut down. TODO: Also
                /// differentiate between 'Connection: close' and a TCP
                /// connection shutdown as the prior implies we can't support
                /// more than one request on a connection while the latter is
                /// probably just a connection timeout.
                graceful: r.is_ok(),
                local: event_receiver.is_closed(),
                http1_rejected_persistence,
            };

            listener.handle_connection_shutdown(details).await;
        }

        event_receiver.close();

        // Notify all unprocessed requests that they were not processed at all.
        while let Ok(request) = event_receiver.try_recv() {
            let _ =
                request
                    .response_handler
                    .try_send(Err(Error::from(crate::v2::ProtocolErrorV2 {
                        code: crate::proto::v2::ErrorCode::REFUSED_STREAM,
                        local: true,
                        message: "Connection closed before request started.".into(),
                    })
                    .into()));
        }

        r
    }

    // TODO: We need both the reader and writer end of the socket to be returned
    // before we can close the connection. (relevant if we ever implement
    // pipelining. The write side may close early if the server only supports
    // returning close delimited bodies).

    // TODO: Most things in here don't request us to fail the entire connection.
    // Also any failure specific to one request should be
    //
    // TODO: Listen for hangup events on the connection (even if we haven't issues a
    // read/write in a while).
    async fn run_inner(
        self: Arc<Self>,
        reader: Box<dyn Readable>,
        mut writer: Box<dyn Writeable>,
        event_receiver: &mut channel::Receiver<ClientConnectionRequest>,
        external_listener: &Option<Box<dyn ConnectionEventListener>>,
        http1_rejected_persistence: &mut bool,
    ) -> Result<()> {
        let mut reader = PatternReader::new(reader, MESSAGE_HEAD_BUFFER_OPTIONS);

        loop {
            let ClientConnectionRequest {
                mut request,
                upgrading,
                response_handler,
            } = match event_receiver.recv().await {
                Ok(v) => v,
                Err(_) => {
                    // Connection was shut down locally.
                    return Ok(());
                }
            };

            let mut request_head = vec![];
            if let Err(e) = Self::prepare_outgoing_request(&mut request, &mut request_head) {
                if let Some(l) = &external_listener {
                    l.handle_request_completed().await;
                }
                let _ = response_handler.try_send(Err(e));
                continue;
            }

            let mut body = encode_request_body_v1(&mut request.head, request.body);

            writer.write_all(&request_head).await?;
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

            let mut persist_connection =
                crate::headers::connection::can_connection_persist(&head.version, &head.headers)?;

            if head.status_code == SWITCHING_PROTOCOLS {
                let _ = response_handler.try_send(Ok(ClientConnectionResponse::Upgrading {
                    response_head: head,
                    reader: Box::new(reader),
                    writer,
                }));
                return Ok(());
            }

            let (body, body_close_delimited) =
                decode_response_body_v1(request.head.method, &head, reader, self.clone()).await?;

            // TODO: Main issue with this is that we can't easily shut down the connection
            // because the client has a hold of the body (if the client doesn't read it we
            // can't close the body).
            let _ = response_handler.try_send(Ok(ClientConnectionResponse::Regular {
                response: Response { head, body },
            }));

            if body_close_delimited {
                persist_connection = false;
                self.event_sender.close();
                // TODO: Cancel all pending unsent requests immediately (so we
                // don't need to wait for this function to return).
                // ^ And before this send the shutting_down event.
            }

            //
            let returned_body = self.return_channel.1.recv().await??;

            if !persist_connection {
                *http1_rejected_persistence = true;
                break;
            }

            reader = match returned_body.wait().await? {
                Some(r) => r,
                None => break,
            };

            if let Some(l) = &external_listener {
                l.handle_request_completed().await;
            }
        }

        Ok(())
    }

    fn prepare_outgoing_request(request: &mut Request, request_head: &mut Vec<u8>) -> Result<()> {
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

        request.head.serialize(request_head)?;

        Ok(())
    }
}

#[async_trait]
impl BodyReturnHandler for ClientConnectionShared {
    async fn handle_returned_body(&self, body: Result<ReturnedBody>) {
        self.event_sender.close();
        let _ = self.return_channel.0.try_send(body);
    }
}
