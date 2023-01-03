use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::sync::Mutex;
use net::backoff::{ExponentialBackoff, ExponentialBackoffOptions};
use parsing::ascii::AsciiString;

use crate::client::client_interface::*;
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

#[derive(Clone)]
pub struct ClientOptions {
    pub backend_balancer: LoadBalancedClientOptions,
}

impl ClientOptions {
    pub fn from_resolver(resolver: Arc<dyn Resolver>) -> Self {
        Self {
            backend_balancer: LoadBalancedClientOptions {
                resolver,
                backend: DirectClientOptions {
                    tls: None,
                    force_http2: false,
                    upgrade_plaintext_http2: false,
                    connection_backoff: ExponentialBackoffOptions {
                        base_duration: Duration::from_millis(100),
                        jitter_duration: Duration::from_millis(200),
                        max_duration: Duration::from_secs(20),
                        cooldown_duration: Duration::from_secs(60),
                        max_num_attempts: 0,
                    },
                    connect_timeout: Duration::from_millis(1000),
                    idle_timeout: Duration::from_secs(2),
                    /// MUST be <= v2::ConnectionOptions::max_enqueued_requests
                    max_outstanding_requests: 100,
                    max_num_connections: 10,
                    http1_max_requests_per_connection: 1,
                    remote_shutdown_is_failure: false,
                    eagerly_connect: true,
                },
                resolver_backoff: ExponentialBackoffOptions {
                    base_duration: Duration::from_millis(100),
                    jitter_duration: Duration::from_millis(200),
                    max_duration: Duration::from_secs(20),
                    cooldown_duration: Duration::from_secs(60),
                    max_num_attempts: 0,
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

        // TODO: Deduplicate this code.
        let secure = match scheme.as_str() {
            "http" => false,
            "https" => true,
            _ => {
                return Err(format_err!("Unsupported scheme: {}", scheme));
            }
        };

        let port = authority.port.unwrap_or(if secure { 443 } else { 80 });

        let resolver = Arc::new(SystemDNSResolver::new(authority.host.clone(), port));

        let mut options = Self::from_resolver(resolver);
        if secure {
            options.backend_balancer.backend.tls = Some(crypto::tls::ClientOptions::recommended());
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
pub struct Client {
    shared: Shared,
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
    /// Note that as soon as a client instance is created it may start creating
    /// a connection to the remote servers.
    ///
    /// Arguments:
    /// - options: Options for how to start connections
    ///
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
        executor::spawn(lb_client.clone().run());

        Ok(Client {
            shared: Shared { options, lb_client },
        })
    }
}

#[async_trait]
impl ClientInterface for Client {
    async fn request(
        &self,
        request: Request,
        request_context: ClientRequestContext,
    ) -> Result<Response> {
        return self
            .shared
            .lb_client
            .request(request, request_context)
            .await;
    }

    async fn current_state(&self) -> ClientState {
        self.shared.lb_client.current_state().await
    }
}
