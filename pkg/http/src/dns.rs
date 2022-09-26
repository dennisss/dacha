use std::ffi::CString;
use std::fs::File;
use std::os::unix::prelude::{AsRawFd, FromRawFd};
use std::ptr::{null, null_mut};

use common::errors::*;
use common::libc;
use common::libc::getaddrinfo;
use net::ip::IPAddress;
use sys::Errno;

use crate::uri::*;

#[derive(Debug, PartialEq)]
pub enum SocketType {
    Stream,
    Datagram,
    Raw,
}

#[derive(Debug, PartialEq)]
pub enum AddressFamily {
    INET,
    INET6,
}

#[derive(Debug)]
pub struct AddrInfo {
    pub family: AddressFamily,
    pub socket_type: SocketType,
    pub address: IPAddress,
}

union SockAddr {
    sockaddr: sys::bindings::sockaddr,
    sockaddr_in: sys::bindings::sockaddr_in,
    sockaddr_in6: sys::bindings::sockaddr_in6,
}

impl From<&IPAddress> for SockAddr {
    fn from(ip: &IPAddress) -> Self {
        let mut sockaddr = SockAddr {
            sockaddr: sys::bindings::sockaddr {
                sa_family: 0,
                sa_data: [0i8; 14],
            },
        };

        match ip {
            IPAddress::V4(ip) => {
                sockaddr.sockaddr.sa_family = sys::bindings::AF_INET as u16;

                let s_addr = unsafe {
                    core::slice::from_raw_parts_mut(
                        core::mem::transmute(&mut sockaddr.sockaddr_in.sin_addr.s_addr),
                        4,
                    )
                };

                s_addr.copy_from_slice(ip.as_ref());
            }
            IPAddress::V6(ip) => {
                sockaddr.sockaddr.sa_family = sys::bindings::AF_INET6 as u16;

                let s6_addr = unsafe {
                    core::slice::from_raw_parts_mut(
                        core::mem::transmute(&mut sockaddr.sockaddr_in6.sin6_addr),
                        16,
                    )
                };

                s6_addr.copy_from_slice(ip.as_ref());
            }
            _ => todo!(),
        }

        sockaddr
    }
}

/// This quickly detects whether or not the local computer's network probably
/// supports connecting to a given ip. This has the primary goal of detecting
/// IPv6 support.
///
/// This depends on the Linux connect() syscall failing if we attempt to connect
/// to an address which is not present in the routing table. We connect() using
/// UDP so it doesn't require any external communication to connect.
///
/// For detecting IPv6, we assume that the computer's connected router does not
/// advertise any IPv6 routes if it hasn't acquired a global IPv6 address. For
/// example, when querying IPv6 routes with the following command we'd except
/// the following results in different scenarios:
///
/// With IPv6 Enabled:
///     $ ip -6 route show dev enp4s0
///     XXXX:XXXX:XXXX:XXXX::/64 proto ra metric 100 pref medium
///     XXXX::/64 proto kernel metric 100 pref medium
///     default via XXXX::XXXX:XXXX:XXXX:XXXX proto ra metric 20100 pref medium
///
///     ^ Has a default route
///
/// With IPv6 disabled on the router:
///     $ ip -6 route show dev enp4s0
///     XXXX:XXXX:XXXX:XXXX::/64 proto ra metric 100 pref medium
///     XXXX::/64 proto kernel metric 100 pref medium
///
///     ^ No default route
pub fn check_ip_routable(ip: &IPAddress) -> Result<bool, Errno> {
    let sockaddr = SockAddr::from(ip);

    let file = unsafe {
        File::from_raw_fd(sys::socket(
            sockaddr.sockaddr.sa_family as i32,
            (sys::bindings::__socket_type::SOCK_DGRAM as i32)
                | (sys::bindings::__socket_type::SOCK_CLOEXEC as i32),
            0,
        )?)
    };

    let r = unsafe {
        sys::connect(
            file.as_raw_fd(),
            core::mem::transmute(&sockaddr),
            core::mem::size_of::<sys::bindings::sockaddr_in6>() as u32,
        )
    };

    match r {
        Ok(()) => Ok(true),
        Err(Errno::ENETUNREACH) => Ok(false),
        Err(e) => Err(e),
    }
}

pub fn lookup_hostname(name: &str) -> Result<Vec<AddrInfo>> {
    let cname = CString::new(name).unwrap();

    let mut addrs: *mut libc::addrinfo = null_mut();
    let ret = unsafe { getaddrinfo(cname.as_ptr(), null(), null(), &mut addrs) };

    // TODO: Use gai_strerror to print the error?
    if ret != 0 {
        return Err(format_err!("Got error {}", ret));
    }

    let mut out = vec![];

    let mut cur_addr = addrs;
    while !cur_addr.is_null() {
        let a = unsafe { *cur_addr };

        // TODO: Validate that this matches the thing in the ai_addr
        let family = match a.ai_family {
            libc::AF_INET => AddressFamily::INET,
            libc::AF_INET6 => AddressFamily::INET6,
            _ => {
                return Err(format_err!("Unknown ai_family: {}", a.ai_family));
            }
        };

        let socket_type = match a.ai_socktype {
            libc::SOCK_STREAM => SocketType::Stream,
            libc::SOCK_DGRAM => SocketType::Datagram,
            libc::SOCK_RAW => SocketType::Raw,
            _ => {
                return Err(format_err!("Unknown ai_socktype: {}", a.ai_socktype));
            }
        };

        // TODO: Validate this using std::mem::size_of for each case based on sa_family
        let addrlen: u32 = a.ai_addrlen;

        // TODO: Should the family always match the one in the addrinfo?
        // TODO: Ensure that the port is not set in these
        assert!(!a.ai_addr.is_null());
        let address = match unsafe { *a.ai_addr }.sa_family as i32 {
            libc::AF_INET => {
                let addr_in = unsafe {
                    *std::mem::transmute::<*const libc::sockaddr, *const libc::sockaddr_in>(
                        a.ai_addr,
                    )
                };

                let data = addr_in.sin_addr.s_addr.to_ne_bytes().to_vec();
                IPAddress::V4(data)
            }
            libc::AF_INET6 => {
                let addr_in6 = unsafe {
                    *std::mem::transmute::<*const libc::sockaddr, *const libc::sockaddr_in6>(
                        a.ai_addr,
                    )
                };

                IPAddress::V6(addr_in6.sin6_addr.s6_addr.to_vec())
            }
            _ => {
                return Err(format_err!("Unsupported family in ai_addr"));
            }
        };

        // TODO: Check a.ai_flags

        if check_ip_routable(&address)? {
            out.push(AddrInfo {
                family,
                socket_type,
                address,
            });
        }

        cur_addr = a.ai_next;
    }

    unsafe {
        libc::freeaddrinfo(addrs);
    };

    Ok(out)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn check_ip_routable_test() -> Result<()> {
        // println!("XXX");
        // let local_ip = http::uri_syntax::parse_ip_literal("192.168.0.1".into())?.0;

        // println!("A");
        // http::dns::check_ip_routable(&local_ip)?;

        let google_aaaa =
            crate::uri_syntax::parse_ip_literal("[2607:f8b0:4005:813::200e]".into())?.0;
        assert_eq!(check_ip_routable(&google_aaaa)?, false);

        let local_v4 = net::ip::IPAddress::V4(vec![192, 168, 0, 1]);
        assert_eq!(check_ip_routable(&local_v4)?, true);

        let loopback = crate::uri_syntax::parse_ip_literal("[::1]".into())?.0;
        println!("{:?}", loopback);
        assert_eq!(check_ip_routable(&loopback)?, true);

        let local_v6 = crate::uri_syntax::parse_ip_literal("[fe80::db3e:1d48:720a:a743]".into())?.0;
        println!("{:x?}", local_v6);
        assert_eq!(check_ip_routable(&local_v6)?, true);

        Ok(())
    }
}
