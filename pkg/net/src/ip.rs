use std::net::{IpAddr, Ipv4Addr};

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
            IPAddress::V6(v) => {
                return Err(err_msg("IPV6 not supported"));
                // TODO: This is wrong. Must parse u16's
                // IpAddr::V6(Ipv6Addr::new(v[0], v[1], v[2], v[3],
                // 						 v[4], v[5], v[6], v[7]))
            }
            IPAddress::VFuture(_) => {
                return Err(err_msg("Future ip address not supported"));
            }
        })
    }
}
