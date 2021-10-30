use std::convert::TryInto;
use std::net::SocketAddr;

use common::errors::*;

use crate::dns::*;
use crate::uri::Host;

#[async_trait]
pub trait Resolver: 'static + Send + Sync {
    async fn resolve(&self) -> Result<Vec<SocketAddr>>;

    async fn wait_for_update(&self);
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
    async fn resolve(&self) -> Result<Vec<SocketAddr>> {
        let mut addrs = vec![];

        match &self.host {
            Host::Name(n) => {
                // TODO: This should become async.
                let raw_addrs = lookup_hostname(n.as_ref())?;

                // TODO: Prefer ipv6 over ipv4 if there are multiple?
                for a in raw_addrs {
                    if a.socket_type == SocketType::Stream {
                        addrs.push(SocketAddr::new(a.address.try_into()?, self.port));
                    }
                }
            }
            Host::IP(ip) => {
                addrs.push(SocketAddr::new(ip.clone().try_into()?, self.port));
            }
        };

        Ok(addrs)
    }

    async fn wait_for_update(&self) {
        // TODO: Implement waiting for the DNS TTL (but if a static ip is given, then
        // naturally we have no work to do).
        common::futures::future::pending().await
    }
}
