use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Debug;
use core::str::FromStr;

use common::errors::*;

use crate::endian::{FromNetworkOrder, ToNetworkOrder};

// TODO: Verify that we aren't able to parse octal ip addresses
// (basically no component should start with a leading 0)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IPAddress {
    V4([u8; 4]),
    V6([u8; 16]),
    // VFuture(Vec<u8>),
}

impl IPAddress {
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8])> {
        crate::ip_syntax::parse_ip(input)
    }
}

impl common::args::ArgType for IPAddress {
    fn parse_raw_arg(raw_arg: common::args::RawArgValue) -> Result<Self>
    where
        Self: Sized,
    {
        let s = match raw_arg {
            common::args::RawArgValue::Bool(_) => {
                return Err(err_msg("Expected string, got bool"));
            }
            common::args::RawArgValue::String(s) => s,
        };

        Ok(s.parse::<Self>()?)
    }
}

impl std::str::FromStr for IPAddress {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let (ip, rest) = Self::parse(s.as_bytes())?;
        if !rest.is_empty() {
            return Err(err_msg("Extra bytes at end of ip address"));
        }

        Ok(ip)
    }
}

impl ToString for IPAddress {
    fn to_string(&self) -> String {
        crate::ip_syntax::serialize_ip(self)
    }
}

impl std::convert::TryFrom<IPAddress> for std::net::IpAddr {
    type Error = Error;

    fn try_from(ip: IPAddress) -> Result<Self> {
        Ok(match ip {
            IPAddress::V4(v) => {
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(v[0], v[1], v[2], v[3]))
            }
            IPAddress::V6(v) => std::net::IpAddr::V6(std::net::Ipv6Addr::from(v)),
            // IPAddress::VFuture(_) => {
            //     return Err(err_msg("Future ip address not supported"));
            // }
        })
    }
}

impl std::convert::From<std::net::IpAddr> for IPAddress {
    fn from(v: std::net::IpAddr) -> Self {
        match v {
            std::net::IpAddr::V4(v) => Self::V4(v.octets()),
            std::net::IpAddr::V6(v) => Self::V6(v.octets()),
        }
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct SocketAddr {
    ip: IPAddress,
    port: u16,
}

impl SocketAddr {
    pub const fn new(ip: IPAddress, port: u16) -> Self {
        Self { ip, port }
    }

    pub fn ip(&self) -> &IPAddress {
        &self.ip
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Debug for SocketAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:{}", self.ip.to_string(), self.port)
    }
}

impl ToString for SocketAddr {
    fn to_string(&self) -> String {
        format!("{}:{}", self.ip.to_string(), self.port)
    }
}

impl std::str::FromStr for SocketAddr {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let (ip, mut rest) = IPAddress::parse(s.as_bytes())?;
        parse_next!(rest, parsing::tag(":"));

        let (port, rest) = crate::ip_syntax::parse_port(rest)?;
        if !rest.is_empty() {
            return Err(err_msg("Extra bytes at end of ip address"));
        }

        let port = port.ok_or_else(|| err_msg("Missing port"))?;

        Ok(Self { ip, port })
    }
}

impl Into<sys::SocketAddr> for SocketAddr {
    fn into(self) -> sys::SocketAddr {
        match self.ip {
            IPAddress::V4(ip) => sys::SocketAddr::ipv4(&ip, self.port.to_network_order()),
            IPAddress::V6(ip) => sys::SocketAddr::ipv6(&ip, self.port.to_network_order()),
            // IPAddress::VFuture(_) => todo!(),
        }
    }
}

impl From<sys::SocketAddr> for SocketAddr {
    fn from(addr: sys::SocketAddr) -> Self {
        if let Some((addr, port)) = addr.as_ipv4() {
            return Self {
                ip: IPAddress::V4(addr),
                port: port.from_network_order(),
            };
        }

        if let Some((addr, port)) = addr.as_ipv6() {
            return Self {
                ip: IPAddress::V6(addr),
                port: port.from_network_order(),
            };
        }

        // TODO: Return an error instead
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_parsing() {
        assert_eq!(
            "127.0.0.1".parse::<IPAddress>().unwrap(),
            IPAddress::V4([127, 0, 0, 1])
        );
    }
}
