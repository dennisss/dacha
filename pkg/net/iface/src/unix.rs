use std::ffi::CStr;
use std::collections::HashMap;

use common::errors::*;
use common::hash::FastHasherBuilder;
use file::{LocalPath, LocalPathBuf};
use net::ip::*;

use crate::types::*;

impl NetworkInterface {
    pub async fn list() -> Result<Vec<Self>> {
        let mut ifaces = HashMap::<String, Self, FastHasherBuilder>::default();

        #[cfg(target_os = "macos")]
        {
            for i in Self::list_basic()? {
                ifaces.insert(i.name.clone(), i);
            }
        }

        for (name, addrs) in Self::getifaddrs()? {
            if !ifaces.contains_key(&name) {
                if cfg!(target_os = "linux") {
                    #[cfg(target_os = "linux")]
                    ifaces.insert(name.clone(), Self::empty_from_name(&name).await?);
                } else {
                    // On macOS, this will not have the loopback interface
                    continue;
                    // return Err(format_err!("Unknown network interface: {}", name));
                }
            }

            let iface = ifaces.get_mut(&name).unwrap();

            if let Some(v) = addrs {
                iface.addrs.push(v);
            }
        }

        Ok(ifaces.into_values().collect())
    }

    // This is split off so that it can run on a single thread safely.
    fn getifaddrs() -> Result<Vec<(String, Option<NetworkInterfaceAddrs>)>> {
        let mut out = vec![];

        let mut addrs: *mut libc::ifaddrs = core::ptr::null_mut();
        let ret = unsafe {
            libc::getifaddrs(&mut addrs)
        };
        if ret != 0 {
            return Err(err_msg("getifaddrs failed!"));
        }

        let mut addr = addrs;
        while addr != core::ptr::null_mut() {
            let item = unsafe { *addr };

            let name = unsafe { CStr::from_ptr(item.ifa_name) }.to_str()?.to_string();

            if item.ifa_addr == core::ptr::null_mut() {
                out.push((name, None)); 
                continue;
            }

            out.push((name, Some(NetworkInterfaceAddrs {
                addr: NetworkInterfaceAddr::parse_libc_sockaddr(unsafe { &*item.ifa_addr }),
                netmask: {
                    if item.ifa_netmask != core::ptr::null_mut() {
                        Some(NetworkInterfaceAddr::parse_libc_sockaddr(unsafe { &*item.ifa_netmask }))
                    } else {
                        None
                    }
                }
            })));

            // TODO: Grab broadcast/p2p addr;

            addr = item.ifa_next;
        }

        unsafe { libc::freeifaddrs(addrs) };

        Ok(out)
    }

    #[cfg(target_os = "linux")]
    async fn empty_from_name(name: &str) -> Result<Self> {
        let typ = NetworkInterfaceType::find(name).await?;
        let index = file::read_to_string(&format!("/sys/class/net/{}/ifindex", name)).await?
            .trim()
            .parse()?;

        Ok(Self {
            index,
            typ,
            name: name.to_string(),
            description: "".into(),
            addrs: vec![]
        })
    } 

}


impl NetworkInterfaceType {
    #[cfg(target_os = "linux")]
    pub async fn find(iface_name: &str) -> Result<Self> {

        let sysfs_dir = LocalPath::new(&format!("/sys/class/net/{}", iface_name)).to_owned();

        if !file::exists(sysfs_dir.join("device")).await? {
            // Some non-physical device.
            return Ok(Self::Unknown);
        }

        if file::exists(sysfs_dir.join("wireless")).await? || file::exists(sysfs_dir.join("phy80211")).await? {
            return Ok(Self::PhysicalWireless);
        }

        Ok(Self::PhysicalEthernet)
    }
}



impl NetworkInterfaceAddr {

    // TODO: Dedup this code with other references to libc::AF_INET
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn parse_libc_sockaddr(addr: &libc::sockaddr) -> Self {
        match addr.sa_family as i32 {
            #[cfg(target_os = "linux")]
            libc::AF_PACKET => {
                // MAC address
                Self::Link
            }
            #[cfg(target_os = "macos")]
            libc::AF_LINK => {
                // MAC address
                Self::Link
            }
            libc::AF_INET => {
                let addr_in = unsafe {
                    *std::mem::transmute::<*const libc::sockaddr, *const libc::sockaddr_in>(
                        addr,
                    )
                };

                Self::IP(IPAddress::V4(addr_in.sin_addr.s_addr.to_ne_bytes()))
            }
            libc::AF_INET6 => {
                let addr_in6 = unsafe {
                    *std::mem::transmute::<*const libc::sockaddr, *const libc::sockaddr_in6>(
                        addr,
                    )
                };

                Self::IP(IPAddress::V6(addr_in6.sin6_addr.s6_addr))
            }
            _ => Self::Unknown
        }
    }
}