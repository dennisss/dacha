use std::time::Duration;

use common::{bytes::Bytes, errors::*};
use net::backoff::*;

use crate::{
    response::BufferedResponse, v2, BodyFromData, Client, ClientInterface, ClientRequestContext,
    Request, RequestHead, ResponseHead,
};

/// Determines whether or not the given http::Client returned error can be
/// safely retried by sending a new request regardless of whether or not the
/// request is idempotent.
///
/// (even non-idempotent requests may be retryable if we know that the server
/// never received it).
pub fn is_client_retryable_error(e: &Error) -> bool {
    if let Some(e) = e.downcast_ref::<v2::ProtocolErrorV2>() {
        if e.is_retryable() {
            return true;
        }
    }

    false
}

pub struct SimpleClientOptions {
    /// Maximum size of responses we will retrieve when storing data in memory.
    pub max_in_memory_response_size: usize,

    pub backoff_options: ExponentialBackoffOptions,
}

impl Default for SimpleClientOptions {
    fn default() -> Self {
        Self {
            max_in_memory_response_size: 10 * 1024 * 1024, // 10MB
            backoff_options: ExponentialBackoffOptions {
                base_duration: Duration::from_millis(10),
                jitter_duration: Duration::from_millis(200),
                max_duration: Duration::from_secs(30),
                cooldown_duration: Duration::from_secs(60),
                max_num_attempts: 5,
            },
        }
    }
}

/*
More needed features:
- Cookies
- Support following Location re-directs.
    - Block infinite loops.
    - Will need to have a layet above Client because we need to be able to create different clients for different hosts.

*/

/*
TODO: Add a test case attempting to connect to an unreachable port on the loal machine. Should timeout after all retries.
*/

/// http::Client wrapper which makes it easier to interact with http endpoints:
/// - Handles retrying of requests.
/// - Decodes Content-Encoding compressed responses.
///
/// TODO: Also add a cookie jar?
pub struct SimpleClient {
    options: SimpleClientOptions,
    client: Box<dyn ClientInterface>,
}

impl SimpleClient {
    pub fn new(client: Client, options: SimpleClientOptions) -> Self {
        Self {
            options,
            client: Box::new(client),
        }
    }

    pub async fn request(
        &self,
        request_head: &RequestHead,
        request_body: Bytes,
        request_context: &ClientRequestContext,
    ) -> Result<BufferedResponse> {
        let mut retry_backoff = ExponentialBackoff::new(self.options.backoff_options.clone());

        let mut num_attempts = 0;

        let mut last_error = Error::from(err_msg("No attempts performed."));
        loop {
            match retry_backoff.start_attempt() {
                ExponentialBackoffResult::Start => {}
                ExponentialBackoffResult::StartAfter(wait_time) => {
                    executor::sleep(wait_time).await.unwrap();
                }
                ExponentialBackoffResult::Stop => break,
            };

            let request = Request {
                head: request_head.clone(),
                body: BodyFromData(request_body.clone()),
            };

            num_attempts += 1;

            let res = match self.request_once(request, request_context.clone()).await {
                Ok(v) => {
                    return Ok(v);
                }
                Err(e) => {
                    last_error = e;
                    // TODO: Also retry GET/HEAD requests (and respect any idempotent marker in the
                    // request context)
                    if !is_client_retryable_error(&last_error) {
                        break;
                    }
                }
            };
        }

        Err(format_err!(
            "Failed after {} attempts. Last error: {:?}",
            num_attempts,
            last_error
        ))
    }

    async fn request_once(
        &self,
        request: Request,
        request_context: ClientRequestContext,
    ) -> Result<BufferedResponse> {
        let mut res = self.client.request(request, request_context).await?;
        let head = res.head;

        if let Some(len) = res.body.len() {
            if len > self.options.max_in_memory_response_size {
                return Err(err_msg("Expected body is too large"));
            }
        }

        let mut buffer = vec![];

        // TODO: Make this optional if the client wants the encoded output?
        let mut body = crate::encoding::decode_content_encoding_body(&head.headers, res.body)?;

        body.read_at_most(&mut buffer, self.options.max_in_memory_response_size)
            .await?;

        let trailers = body.trailers().await?;

        Ok(BufferedResponse {
            head,
            body: buffer.into(),
            trailers,
        })
    }

    // pub fn get()
}
