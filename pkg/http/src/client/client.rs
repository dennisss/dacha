use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use common::async_std::net::TcpStream;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::errors::*;
use common::io::{Readable, Writeable};
use parsing::ascii::AsciiString;

use crate::backoff::{ExponentialBackoff, ExponentialBackoffOptions};
use crate::client::client_interface::ClientInterface;
use crate::client::direct_client::DirectClientOptions;
use crate::client::load_balanced_client::{LoadBalancedClient, LoadBalancedClientOptions};
use crate::client::resolver::{Resolver, SystemDNSResolver};
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

/*
Top level responsibilities:
- Retry retryable failures
-
*/

/*
TODO: Connections should have an accepting_connections()

    We need information on accepting_connections() in
*/

// TODO: If we recieve an unterminated body, then we should close the
// connection right afterwards.

pub struct ClientOptions {
    pub max_num_retries: usize,

    pub retry_backoff: ExponentialBackoffOptions,

    pub backend_balancer: LoadBalancedClientOptions,
}

impl ClientOptions {
    pub fn from_resolver(resolver: Arc<dyn Resolver>) -> Self {
        Self {
            max_num_retries: 5,
            retry_backoff: ExponentialBackoffOptions {
                base_duration: Duration::from_millis(10),
                jitter_duration: Duration::from_millis(200),
                max_duration: Duration::from_secs(30),
                cooldown_duration: Duration::from_secs(60),
            },
            backend_balancer: LoadBalancedClientOptions {
                resolver,
                backend: DirectClientOptions {
                    tls: None,
                    force_http2: false,
                    connection_backoff: ExponentialBackoffOptions {
                        base_duration: Duration::from_millis(100),
                        jitter_duration: Duration::from_millis(200),
                        max_duration: Duration::from_secs(20),
                        cooldown_duration: Duration::from_secs(60),
                    },
                    connect_timeout: Duration::from_millis(500),
                    idle_timeout: Duration::from_secs(2),
                },
                resolver_backoff: ExponentialBackoffOptions {
                    base_duration: Duration::from_millis(100),
                    jitter_duration: Duration::from_millis(200),
                    max_duration: Duration::from_secs(20),
                    cooldown_duration: Duration::from_secs(60),
                },
                subset_size: 10,
            },
        }
    }

    pub fn from_uri(uri: &Uri) -> Result<Self> {
        let authority = uri
            .authority
            .clone()
            .ok_or_else(|| err_msg("Uri missing an authority"))?;

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

        let port = authority.port.unwrap_or(if secure { 443 } else { 80 });

        // TODO: Explicitly check that the port fits within a u16.
        let resolver = Arc::new(SystemDNSResolver::new(authority.host.clone(), port as u16));

        let mut options = Self::from_resolver(resolver);
        if secure {
            options.backend_balancer.backend.tls =
                Some(crypto::tls::options::ClientOptions::recommended());
        }

        Ok(options)
    }

    pub fn set_force_http2(mut self, value: bool) -> Self {
        self.backend_balancer.backend.force_http2 = value;
        self
    }
}

impl TryFrom<Uri> for ClientOptions {
    type Error = Error;

    fn try_from(value: Uri) -> Result<Self> {
        Self::from_uri(&value)
    }
}

impl TryFrom<&str> for ClientOptions {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        let uri = value.parse()?;
        Self::from_uri(&uri)
    }
}

/// HTTP client connected to a single server.
///
/// TODO: When the Client is dropped, we know that no more requests will be made
/// so we should initiate the shutdown of internal connections.
#[derive(Clone)]
pub struct Client {
    shared: Arc<Shared>,
}

struct Shared {
    // /// Uri to which we should connection.
    // /// This should only a scheme and authority.
    // base_uri: Uri,
    options: ClientOptions,

    lb_client: LoadBalancedClient,
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
    pub fn create<E: Into<Error> + Send, O: TryInto<ClientOptions, Error = E>>(
        options: O,
    ) -> Result<Self> {
        let options = options.try_into().map_err(|e| e.into())?;
        Self::create_impl(options)
    }

    fn create_impl(options: ClientOptions) -> Result<Self> {
        let lb_client = LoadBalancedClient::new(options.backend_balancer.clone());
        task::spawn(lb_client.clone().run());

        Ok(Client {
            shared: Arc::new(Shared { options, lb_client }),
        })
    }
}

#[async_trait]
impl ClientInterface for Client {
    async fn request(&self, request: Request) -> Result<Response> {
        // TODO: Retrying requires that we can reset the HTTP body.

        return self.shared.lb_client.request(request).await;

        /*
        let mut retry_backoff = ExponentialBackoff::new(self.shared.options.retry_backoff.clone());

        for _ in 0..self.shared.options.max_num_retries {
            if let Some(wait_time) = retry_backoff.start_attempt() {
                task::sleep(wait_time).await;
            }

            match self.shared.lb_client.request(request).await {
                Ok(v) => {
                    return Ok(v);
                }
                Err(e) => {
                    let mut retryable = false;
                    if let Some(e) = e.downcast_ref::<v2::ProtocolErrorV2>() {
                        if e.is_retryable() {
                            retryable = true;
                        }
                    }

                    if retryable {
                        retry_backoff.end_attempt(false);
                        // continue;
                    }

                    return Err(e);
                }
            }
        }

        Err(err_msg("Exceeded max num request retries"))
        */
    }
}
