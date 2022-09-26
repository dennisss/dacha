use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use common::errors::*;

// TODO: Verify that we aren't able to parse octal ip addresses
// (basically no component should start with a leading 0)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IPAddress {
    V4(Vec<u8>),
    V6(Vec<u8>),
    VFuture(Vec<u8>),
}

impl std::convert::TryFrom<IPAddress> for IpAddr {
    type Error = Error;

    fn try_from(ip: IPAddress) -> Result<Self> {
        Ok(match ip {
            IPAddress::V4(v) => IpAddr::V4(Ipv4Addr::new(v[0], v[1], v[2], v[3])),
            IPAddress::V6(v) => IpAddr::V6(Ipv6Addr::from(*array_ref![&v, 0, 16])),
            IPAddress::VFuture(_) => {
                return Err(err_msg("Future ip address not supported"));
            }
        })
    }
}

impl std::convert::From<IpAddr> for IPAddress {
    fn from(v: IpAddr) -> Self {
        match v {
            IpAddr::V4(v) => Self::V4(v.octets().to_vec()),
            IpAddr::V6(v) => Self::V6(v.octets().to_vec()),
        }
    }
}
