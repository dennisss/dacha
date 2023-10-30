use std::sync::Arc;
use std::{collections::HashMap, time::Duration};

use common::{bytes::Bytes, errors::*};
use executor::sync::Mutex;
use net::backoff::*;
use parsing::ascii::AsciiString;

use crate::{
    response::BufferedResponse, v2, BodyFromData, Client, ClientInterface, ClientOptions,
    ClientRequestContext, Request, RequestHead, ResponseHead,
};

use super::load_balanced_client::LoadBalancedClientOptions;

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
            max_in_memory_response_size: 20 * 1024 * 1024, // 20MiB
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
- Caching responses / doing If-Not-Modified- stuff.
- Streaming download of a file using byte ranges
    - Can intelligently download just
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
    /// TODO: Clean up clients if they haven't been used in a while.
    clients: Mutex<HashMap<String, Arc<dyn ClientInterface>>>,
}

impl SimpleClient {
    pub fn new(options: SimpleClientOptions) -> Self {
        Self {
            options,
            clients: Mutex::new(HashMap::new()),
        }
    }

    pub async fn request(
        &self,
        request_head: &RequestHead,
        request_body: Bytes,
        request_context: &ClientRequestContext,
    ) -> Result<BufferedResponse> {
        let client = {
            let mut clients = self.clients.lock().await;

            if request_head.uri.scheme.is_none() || request_head.uri.authority.is_none() {
                return Err(err_msg("No schema/authority specified for request."));
            }

            let host_uri = crate::uri::Uri {
                scheme: request_head.uri.scheme.clone(),
                authority: request_head.uri.authority.clone(),
                path: AsciiString::new(""),
                query: None,
                fragment: None,
            };

            let key = host_uri.to_string()?;

            if !clients.contains_key(&key) {
                let client = Client::create(ClientOptions::from_uri(&host_uri)?).await?;
                clients.insert(key.clone(), Arc::new(client));
            }

            clients.get(&key).unwrap().clone()
        };

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

            let res = match self
                .request_once(client.as_ref(), request, request_context.clone())
                .await
            {
                Ok(v) => {
                    // tODO: Also retry some HTTP codes.

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
        client: &dyn ClientInterface,
        request: Request,
        request_context: ClientRequestContext,
    ) -> Result<BufferedResponse> {
        let mut res = client.request(request, request_context).await?;
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
