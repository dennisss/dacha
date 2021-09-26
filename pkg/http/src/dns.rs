use std::ffi::CString;
use std::ptr::{null, null_mut};

use common::errors::*;
use common::libc;
use common::libc::getaddrinfo;
use net::ip::IPAddress;

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

        out.push(AddrInfo {
            family,
            socket_type,
            address,
        });

        cur_addr = a.ai_next;
    }

    unsafe {
        libc::freeaddrinfo(addrs);
    };

    Ok(out)
}
