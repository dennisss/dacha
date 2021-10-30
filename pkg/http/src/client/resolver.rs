use std::convert::TryInto;
use std::net::SocketAddr;

use common::errors::*;

use crate::dns::*;
use crate::uri::{Authority, Host};

/// Function which is called whenever the resolver has a change in the set of
/// endpoints resolved by the next call to resolve().
///
/// Returns whether or not the listener should continue to be called on future
/// changes.
pub type ResolverChangeListener = Box<dyn Fn() -> bool + Send + Sync + 'static>;

#[async_trait]
pub trait Resolver: 'static + Send + Sync {
    async fn resolve(&self) -> Result<Vec<ResolvedEndpoint>>;

    async fn add_change_listener(&self, listener: ResolverChangeListener);
}

/// Single unique target at which a service can be reached.
///
/// NOTE: We assume that all endpoints of a single service use a common protocol
/// (e.g. HTTP over TLS).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ResolvedEndpoint {
    /// IP address and port to which we should connect.
    pub address: SocketAddr,

    /// Host name and port to advertise to the connected service (this will
    /// configure the HTTP 'Host' header and the TLS host extension).
    ///
    /// No attempts will be made to resolve this to a new ip address as we
    /// assume that has already been done.
    ///
    /// NOTE: The port in this field should always equal to the one in
    /// 'address'.
    pub authority: Authority,
}

pub struct SystemDNSResolver {
    host: Host,
    port: u16,
}

impl SystemDNSResolver {
    pub fn new(host: Host, port: u16) -> Self {
        Self { host, port }
    }
}

#[async_trait]
impl Resolver for SystemDNSResolver {
    async fn resolve(&self) -> Result<Vec<ResolvedEndpoint>> {
        let mut endpoints = vec![];

        let authority = Authority {
            user: None,
            host: self.host.clone(),
            port: Some(self.port as usize),
        };

        match &self.host {
            Host::Name(n) => {
                // TODO: This should become async.
                let addrs = lookup_hostname(n.as_ref())?;

                // TODO: Prefer ipv6 over ipv4 if there are multiple?
                for a in addrs {
                    if a.socket_type == SocketType::Stream {
                        endpoints.push(ResolvedEndpoint {
                            address: SocketAddr::new(a.address.try_into()?, self.port),
                            authority: authority.clone(),
                        });
                    }
                }
            }
            Host::IP(ip) => {
                endpoints.push(ResolvedEndpoint {
                    address: SocketAddr::new(ip.clone().try_into()?, self.port),
                    authority,
                });
            }
        };

        Ok(endpoints)
    }

    async fn add_change_listener(&self, listener: ResolverChangeListener) {
        // TODO: Implement waiting for the DNS TTL (but if a static ip is given,
        // then naturally we have no work to do).
    }
}
