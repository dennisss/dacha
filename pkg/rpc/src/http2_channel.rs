use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::marker::PhantomData;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use common::io::IoError;
use common::io::IoErrorKind;
use common::io::Readable;
use executor::cancellation::CancellationToken;
use executor::channel::spsc;
use executor_multitask::{ServiceResource, ServiceResourceSubscriber};
use http::header::*;
use http::headers::content_type::MediaType;
use http::ClientInterface;
use net::backoff::ExponentialBackoff;

use crate::channel::Channel;
use crate::client_types::*;
use crate::constants::{GRPC_ACCEPT_ENCODING, GRPC_ENCODING};
use crate::credentials::ChannelCredentialsProvider;
use crate::media_type::*;
use crate::message::MessageReader;
use crate::message_request_body::MessageRequestBody;
use crate::message_request_body::MessageRequestBuffer;
use crate::metadata::*;
use crate::status::*;
use crate::RetryingOptions;

pub struct Http2ChannelOptions {
    pub http: http::ClientOptions,

    pub credentials: Option<Box<dyn ChannelCredentialsProvider>>,

    pub retrying: Option<RetryingOptions>,

    pub max_request_buffer_size: usize,

    pub response_interceptor: Option<Arc<dyn ClientResponseInterceptor>>,
}

impl TryFrom<http::ClientOptions> for Http2ChannelOptions {
    type Error = Error;

    fn try_from(value: http::ClientOptions) -> Result<Self> {
        Ok(Self {
            http: value,
            retrying: Some(RetryingOptions::default()),
            max_request_buffer_size: 16 * 1024, // 16KB
            credentials: None,
            response_interceptor: None,
        })
    }
}

impl TryFrom<&str> for Http2ChannelOptions {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        http::ClientOptions::try_from(value)?.try_into()
    }
}

pub trait TryIntoHttp2ChannelOptions {
    fn try_into_value(self) -> Result<Http2ChannelOptions>;
}

impl TryIntoResult<Http2ChannelOptions> for &str {
    fn try_into_result(self) -> Result<Http2ChannelOptions> {
        self.try_into()
    }
}

impl TryIntoResult<Http2ChannelOptions> for http::ClientOptions {
    fn try_into_result(self) -> Result<Http2ChannelOptions> {
        self.try_into()
    }
}

/*
There are three cases to retry:

1. HTTPv2 level errors (usually will happen before the response headers are available)
    - Have the HTTPv2 layer handle retrying these.
2. Trailer-Only failure (before the body is received)
    - Can be retried for all requests

3. unary-unary
    - buffer_full_response request context hint.

TODO: Implement full unary response retrying (e.g. before we get back the full response)
*/

/*
I want to provide a Content-Length hint for basic requests
- Issue is that we insist on starting the request immediately before the client can send some data.
*/

pub struct Http2Channel {
    shared: Arc<Shared>,
}

struct Shared {
    client: http::Client,
    options: Http2ChannelOptions,
}

// Passthrough to self.shared.client
#[async_trait]
impl ServiceResource for Http2Channel {
    async fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>) {
        self.shared.client.add_cancellation_token(token).await
    }

    async fn new_resource_subscriber(&self) -> Box<dyn ServiceResourceSubscriber> {
        self.shared.client.new_resource_subscriber().await
    }
}

impl Http2Channel {
    pub async fn create<O: TryIntoResult<Http2ChannelOptions>>(options: O) -> Result<Self> {
        let options = options.try_into_result()?;
        let client = http::Client::create(options.http.clone().set_force_http2(true)).await?;

        Ok(Self {
            shared: Arc::new(Shared { client, options }),
        })
    }

    async fn call_raw_impl(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
        request_receiver: spsc::Receiver<Result<Option<Bytes>>>,
    ) -> ClientStreamingResponse<()> {
        // TODO: Tune the length.
        let buffer = Arc::new(MessageRequestBuffer::new(
            self.shared.options.max_request_buffer_size,
            request_receiver,
        ));

        // TODO: Apply this later as authorization metadata may need to be refreshed
        // between retries?
        let mut request_context = request_context.clone();
        if let Some(creds) = self.shared.options.credentials.as_ref() {
            if let Err(e) = creds
                .attach_request_credentials(service_name, method_name, &mut request_context)
                .await
            {
                return ClientStreamingResponse::from_error(e);
            }
        }

        let request_sender = Http2RequestSender {
            shared: self.shared.clone(),
            // TODO: Add the full package path.
            path: format!("/{}/{}", service_name, method_name),
            request_context,
            request_buffer: buffer,
        };

        // TODO: Need to implement custom logic for retrying service RPC errors (some
        // GRPC statuses should only be returned by the service exclusively).
        // let future_response = async move { client.request(request,
        // http_request_context).await };
        ClientStreamingResponse::from_future_response(
            async move { request_sender.send_request().await },
        )
    }

    /// This is mainly exported to be used by the LocalChannel to emulate an
    /// HTTP2 channel.
    pub(crate) async fn process_existing_response(
        response: Result<http::Response>,
        attempt_alive: spsc::Sender<()>,
        request_context: &ClientRequestContext,
    ) -> Box<dyn ClientStreamingResponseInterface> {
        let mut output = Http2ClientStreamingResponse::default();

        let response = match response {
            Ok(v) => v,
            Err(e) => {
                output.set_error(e);
                return Box::new(output);
            }
        };

        if let Err(e) = Http2RequestSender::process_single_response(
            response,
            request_context,
            false,
            &mut output,
        )
        .await
        {
            output.set_error(e);
            return Box::new(output);
        }

        output.attempt_alive = Some(attempt_alive);
        Box::new(output)
    }
}

#[async_trait]
impl Channel for Http2Channel {
    async fn call_raw(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &ClientRequestContext,
    ) -> (ClientStreamingRequest<()>, ClientStreamingResponse<()>) {
        // NOTE: This must be bounded to ensure there is backpressure while the
        // connection is sending packets.
        //
        // Must be able to hold at 2 entries (1 for a unary message and 1 for the end of
        // stream message).
        //
        // TODO: Improve the tuning on this bound.
        let (request_sender, request_receiver) = spsc::bounded(2);
        let request = ClientStreamingRequest::new(request_sender);

        let mut request_context = request_context.clone();
        // TODO: Merge with any interceptor already set by the user.
        request_context.response_interceptor = self.shared.options.response_interceptor.clone();

        // TODO: Pass in the full request_context without borrowing here.
        let mut response = self
            .call_raw_impl(
                service_name,
                method_name,
                &request_context,
                request_receiver,
            )
            .await;

        if let Some(interceptor) = &request_context.response_interceptor {
            response.set_interceptor(interceptor.clone());
        }

        (request, response)
    }
}

/// Worker for sending a single RPC over HTTP2.
struct Http2RequestSender {
    shared: Arc<Shared>,

    path: String,
    request_context: ClientRequestContext,
    request_buffer: Arc<MessageRequestBuffer>,
}

struct Retrier<'a> {
    options: &'a RetryingOptions,
    backoff: ExponentialBackoff,
    num_local_error_retries: usize,
}

impl Http2RequestSender {
    async fn send_request(&self) -> Box<dyn ClientStreamingResponseInterface> {
        // TODO: Somewhere we want to return an Internal error if we run out of retries.
        let mut retrier = self.shared.options.retrying.as_ref().map(|options| {
            let mut backoff = ExponentialBackoff::new(options.backoff.clone());
            // Start first attempt (will never require backoff).
            let _ = backoff.start_attempt();

            Retrier {
                options,
                backoff,
                num_local_error_retries: 0,
            }
        });

        loop {
            let mut response = Http2ClientStreamingResponse::default();

            let mut error = match self
                .send_single_request(retrier.is_some(), &mut response)
                .await
            {
                Ok(v) => {
                    return Box::new(response);
                }
                Err(e) => e,
            };

            response.attempt_alive = None;
            assert!(response.state.is_none());

            let retry = {
                if let Some(retrier) = &mut retrier {
                    self.should_retry(&mut error, retrier).await
                } else {
                    false
                }
            };

            if retry {
                if let Some(interceptor) = &self.request_context.response_interceptor {
                    interceptor
                        .on_response_complete(false, &response.context)
                        .await;
                }

                continue;
            }

            response.set_error(error);
            return Box::new(response);
        }
    }

    async fn should_retry(&self, error: &mut Error, retrier: &mut Retrier<'_>) -> bool {
        let mut is_retryable = false;

        // Check if we saw an HTTP2 error that indicates that the server never got the
        // request (these are always retryable).
        if let Some(error) = error.downcast_ref::<http::v2::ProtocolErrorV2>() {
            if error.is_retryable() {
                is_retryable = true;

                if error.local
                    && retrier.num_local_error_retries < retrier.options.max_local_error_retries
                {
                    retrier.num_local_error_retries += 1;
                    return true;
                }
            }
        }

        let code = match error.downcast_ref::<Status>() {
            Some(status) => status.code(),
            None => StatusCode::Unknown,
        };

        // Check for code based retryability. Only applies to idempotent requests.
        is_retryable |=
            self.request_context.idempotent && retrier.options.retryable_codes.contains(&code);

        is_retryable &= self.request_buffer.is_retryable().await;

        if is_retryable {
            let have_remaining_attempts = {
                retrier.backoff.end_attempt(false);
                match retrier.backoff.start_attempt() {
                    net::backoff::ExponentialBackoffResult::Start => true,
                    net::backoff::ExponentialBackoffResult::StartAfter(duration) => {
                        executor::sleep(duration).await.unwrap();
                        true
                    }
                    net::backoff::ExponentialBackoffResult::Stop => false,
                }
            };

            if have_remaining_attempts {
                // eprintln!(
                //     "[rpc::Http2Channel] [{}] Retrying error {}",
                //     self.path, error
                // );
                return true;
            }

            *error = Status::internal(format!(
                "Exceeded maximum number of attempts in RPC. Last error: {}",
                error
            ))
            .into();
        }

        false
    }

    /// Attempts to issue a single HTTP2 request to fulfill the request.
    ///
    /// This function should either:
    /// - Construct 'output' into a valid non-Error state that can be handled
    ///   off to a client and return Ok(()).
    /// - Return an error that will be propagated back to the user.
    ///
    /// For the purposes of retrying, any part of the request not executed in
    /// this function will be considered to be non-retryable.
    async fn send_single_request(
        &self,
        might_retry: bool,
        output: &mut Http2ClientStreamingResponse,
    ) -> Result<()> {
        let (attempt_alive_sender, attempt_alive_receiver) = spsc::bounded(0);
        output.attempt_alive = Some(attempt_alive_sender);

        // Reset the context metadata for this attempt.
        output.context = ClientResponseContext::default();

        let body = Box::new(MessageRequestBody::new(
            self.request_buffer.clone(),
            attempt_alive_receiver,
        ));

        // TODO: Use GET for methods known to be idempotnet (and doesn't have a request
        // body?).
        let mut request = http::RequestBuilder::new()
            .method(http::Method::POST)
            .path(&self.path)
            // TODO: No gurantee that we were given proto data.
            .header(
                CONTENT_TYPE,
                RPCMediaType {
                    protocol: RPCMediaProtocol::Default,
                    serialization: RPCMediaSerialization::Proto,
                }
                .to_string(),
            )
            .header(GRPC_ENCODING, "identity")
            .header(GRPC_ACCEPT_ENCODING, "identity")
            .accept_trailers(true)
            .body(body)
            .build()
            .map_err(|e| {
                Status::invalid_argument(format!("Failed to build an HTTP request: {}", e))
            })?;

        self.request_context
            .metadata
            .append_to_headers(&mut request.head.headers)
            .map_err(|e| {
                Status::invalid_argument(format!(
                    "Failed to append HTTP head metadata headers: {}",
                    e,
                ))
            })?;

        let mut http_request_context = http::ClientRequestContext::default();
        http_request_context = self.request_context.http.clone();

        let mut http_response_context = output
            .context
            .http_response_context
            .get_or_insert_with(|| http::ClientResponseContext::default());

        let mut response = self
            .shared
            .client
            .request(request, http_request_context, http_response_context)
            .await?;

        Self::process_single_response(response, &self.request_context, might_retry, output).await
    }

    async fn process_single_response(
        mut response: http::Response,
        request_context: &ClientRequestContext,
        might_retry: bool,
        output: &mut Http2ClientStreamingResponse,
    ) -> Result<()> {
        // Separation point where we can

        if response.head.status_code != http::status_code::OK {
            return Err(crate::Status::unknown("Server responded with non-OK status").into());
        }

        let response_type = RPCMediaType::parse(&response.head.headers)
            .ok_or_else(|| err_msg("Response received without valid content type"))?;
        if response_type.protocol != RPCMediaProtocol::Default
            || response_type.serialization != RPCMediaSerialization::Proto
        {
            return Err(err_msg("Received unsupported media type"));
        }

        output.context.metadata.head_metadata = Metadata::from_headers(&response.head.headers)?;

        // Handle trailers only mode.
        // In this mode, the status is propagated through the main headers and no body
        // or trailers should be present.
        //
        // !response.body.has_trailers() also implies that the END_STREAM bit was set of
        // the HTTP2 HEADERS frame rather than there being an extra empty DATA frame.
        // This means it is safe for us to just ignore reading the body.
        let is_trailers_only = response.body.len() == Some(0) && !response.body.has_trailers();
        let has_status_headers = Status::has_headers(&response.head.headers);
        if has_status_headers != is_trailers_only {
            return Err(err_msg("Response contained malformed Trailer-Only form."));
        }
        if has_status_headers {
            match Status::from_headers(&response.head.headers)?.into_result() {
                Ok(()) => {
                    output.state = Some(Http2ClientStreamingResponseState::Result(Ok(())));
                    return Ok(());
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // Receive the body.
        // TODO: Limit the max buffer size here.
        // TODO: If a client calls recv_head() this may cause infinite blocking.
        if request_context.buffer_full_response && might_retry {
            while let Some(message) = Self::read_message(response.body.as_mut()).await? {
                output.buffered_messages.push_back(message);
            }

            Self::read_trailers(
                response.body.as_mut(),
                &mut output.context.metadata.trailer_metadata,
            )
            .await?;

            output.state = Some(Http2ClientStreamingResponseState::Result(Ok(())));
            return Ok(());
        }

        // Otherwise, we will defer to the caller to poll the rest of the request.

        output.state = Some(Http2ClientStreamingResponseState::ReceivingMessages(
            response.body,
        ));
        Ok(())
    }

    async fn read_message(body: &mut dyn http::Body) -> Result<Option<Bytes>> {
        let mut reader = MessageReader::new(body);
        if let Some(message) = reader.read().await? {
            if message.is_trailers {
                return Err(err_msg("Did not expect a trailers message"));
            }

            Ok(Some(message.data))
        } else {
            Ok(None)
        }
    }

    async fn read_trailers(
        body: &mut dyn http::Body,
        trailer_metadata: &mut Metadata,
    ) -> Result<()> {
        let trailers = body
            .trailers()
            .await?
            .ok_or_else(|| err_msg("Server responded without trailers"))?;

        *trailer_metadata = Metadata::from_headers(&trailers)?;

        Status::from_headers(&trailers)?.into_result()
    }
}

// TODO: Need to remap all returnable errors in this to rpc::Status (especially
// ProtocolErrorV2).
#[derive(Default)]
struct Http2ClientStreamingResponse {
    context: ClientResponseContext,

    /// Already received messages which haven't been handled off to a client
    /// yet.
    buffered_messages: VecDeque<Bytes>,

    state: Option<Http2ClientStreamingResponseState>,

    // TODO: Drop me once we enter the Result state.
    attempt_alive: Option<spsc::Sender<()>>,
}

enum Http2ClientStreamingResponseState {
    ReceivingMessages(Box<dyn http::Body>),

    ReceivingTrailers(Box<dyn http::Body>),

    /// We are done receiving everything and got the given result.
    Result(Result<()>),
}

impl Http2ClientStreamingResponse {
    fn set_error(&mut self, error: Error) {
        self.state = Some(Http2ClientStreamingResponseState::Result(Err(error)));
        self.buffered_messages.clear();
    }
}

#[async_trait]
impl ClientStreamingResponseInterface for Http2ClientStreamingResponse {
    async fn recv_bytes(&mut self) -> Option<Bytes> {
        // NOTE: If we enter the Result(Err(_)) state, we will clear this to avoid
        // receiving messages when there is an error.
        if let Some(data) = self.buffered_messages.pop_front() {
            return Some(data);
        }

        let state = match self.state.take() {
            Some(v) => v,
            None => return None,
        };

        match state {
            Http2ClientStreamingResponseState::ReceivingMessages(mut body) => {
                match Http2RequestSender::read_message(body.as_mut()).await {
                    Ok(Some(value)) => {
                        self.state =
                            Some(Http2ClientStreamingResponseState::ReceivingMessages(body));
                        Some(value)
                    }
                    Ok(None) => {
                        self.state =
                            Some(Http2ClientStreamingResponseState::ReceivingTrailers(body));
                        None
                    }
                    Err(e) => {
                        // TODO: We may miss a failure if the read_message future is cancelled
                        // before we get here.
                        self.state = Some(Http2ClientStreamingResponseState::Result(Err(e)));
                        None
                    }
                }
            }
            Http2ClientStreamingResponseState::ReceivingTrailers(_)
            | Http2ClientStreamingResponseState::Result(_) => {
                self.state = Some(state);
                None
            }
        }
    }

    async fn finish(&mut self) -> Result<()> {
        let state = self
            .state
            .take()
            .ok_or_else(|| err_msg("Response in invalid state"))?;

        match state {
            Http2ClientStreamingResponseState::ReceivingMessages(body) => {
                Err(err_msg("Response body hasn't been fully read yet"))
            }
            Http2ClientStreamingResponseState::ReceivingTrailers(mut body) => {
                Http2RequestSender::read_trailers(
                    body.as_mut(),
                    &mut self.context.metadata.trailer_metadata,
                )
                .await
            }
            Http2ClientStreamingResponseState::Result(result) => result,
        }
    }

    fn context(&self) -> &ClientResponseContext {
        &self.context
    }
}
