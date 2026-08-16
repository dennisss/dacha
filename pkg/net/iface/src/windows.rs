use std::alloc::{alloc, dealloc, Layout};

use windows::Win32::Foundation::NO_ERROR;
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GET_ADAPTERS_ADDRESSES_FLAGS, IP_ADAPTER_ADDRESSES_LH,
    IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL,
};
use windows::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN, SOCKADDR_IN6
};

use common::errors::*;
use net::ip::*;

use crate::types::*;

#[cfg(target_os = "windows")]
unsafe fn read_wide_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    String::from_utf16_lossy(slice)
}

impl NetworkInterface {
    pub async fn list() -> Result<Vec<Self>> {
        let mut out_buf_len: u32 = 0;

        // Initial size check
        unsafe {
            let _ = GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                GET_ADAPTERS_ADDRESSES_FLAGS(0),
                None,
                None,
                &mut out_buf_len,
            );
        }

        if out_buf_len == 0 {
            return Ok(vec![]);
        }

        // TODO: ceil_div the size.
        let mut buffer = vec![0u64; ((out_buf_len as usize) / 8) + 1];

        // Fetch the data
        let res = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                GET_ADAPTERS_ADDRESSES_FLAGS(0),
                None,
                Some(buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut out_buf_len,
            )
        };

        if res != NO_ERROR.0 {
            return Err(format_err!("GetAdaptersAddresses failed with error code: {}", res));
        }

        let mut out = vec![];

        let mut current = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        
        while !current.is_null() {
            let adapter = unsafe { &*current };
            let name = unsafe { read_wide_string(adapter.FriendlyName.0) };
            let description = unsafe { read_wide_string(adapter.Description.0) };

            let typ = match adapter.IfType {
                IF_TYPE_ETHERNET_CSMACD => NetworkInterfaceType::PhysicalEthernet,
                IF_TYPE_IEEE80211 => NetworkInterfaceType::PhysicalWireless,
                IF_TYPE_SOFTWARE_LOOPBACK => NetworkInterfaceType::Loopback,
                IF_TYPE_TUNNEL => NetworkInterfaceType::Tunnel,
                _ => NetworkInterfaceType::Unknown,
            };

            let mut addrs = vec![];

            let ipv4_index = unsafe { adapter.Anonymous1.Anonymous.IfIndex };
            let ipv6_index = adapter.Ipv6IfIndex;

            let mac_len = adapter.PhysicalAddressLength as usize;
            if mac_len > 0 {
                let mac_bytes = &adapter.PhysicalAddress[..mac_len];
                addrs.push(NetworkInterfaceAddrs {
                    addr: NetworkInterfaceAddr::Link2(mac_bytes.to_vec()),
                    netmask: None
                });
            }

            // Iterate over all IP addresses assigned to this interface
            let mut unicast = adapter.FirstUnicastAddress;
            while !unicast.is_null() {
                unsafe {
                    let addr_struct = &*unicast;
                    let sockaddr = addr_struct.Address.lpSockaddr;
                    
                    if !sockaddr.is_null() {
                        let family = (*sockaddr).sa_family;
                        let prefix_len = addr_struct.OnLinkPrefixLength;

                        if family == AF_INET {
                            let sa_in = &*(sockaddr as *const SOCKADDR_IN);
                            let ip_bytes = sa_in.sin_addr.S_un.S_un_b;

                            let mask = u32::MAX.checked_shl(32 - prefix_len as u32).unwrap_or(0);
                            let mask_bytes = mask.to_be_bytes();

                            addrs.push(NetworkInterfaceAddrs {
                                addr: NetworkInterfaceAddr::IP(IPAddress::V4([
                                    ip_bytes.s_b1, ip_bytes.s_b2, ip_bytes.s_b3, ip_bytes.s_b4
                                ])),
                                netmask: Some(NetworkInterfaceAddr::IP(IPAddress::V4(mask_bytes)))
                            });
                            
                        } else if family == AF_INET6 {
                            // Extract IPv6
                            let sa_in6 = &*(sockaddr as *const SOCKADDR_IN6);
                            let ip_bytes = sa_in6.sin6_addr.u.Byte;

                            // TODO: Include prefix_len

                            addrs.push(NetworkInterfaceAddrs {
                                addr: NetworkInterfaceAddr::IP(IPAddress::V6(ip_bytes)),
                                netmask: None
                            });
                        }
                    }
                }
                unicast = unsafe { (*unicast).Next };
            }

            out.push(NetworkInterface {
                index: ipv4_index,
                name,
                description,
                typ,
                addrs
            });

            current = adapter.Next;
        }

        Ok(out)
    }
}