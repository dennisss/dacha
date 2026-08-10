use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::FromRawFd;
use std::sync::atomic::AtomicUsize;

use base_util::null_terminated::read_null_terminated_string;
use common::errors::*;
use sys::{socket, AddressFamily};

use crate::ip::IPAddress;
use crate::udp::MessageSocket;

/*

Must use these macros to access things:
https://man7.org/linux/man-pages/man3/netlink.3.html

NETLINK_ROUTE
    https://man7.org/linux/man-pages/man7/rtnetlink.7.html


*/

struct NetlinkSocket {
    inner: MessageSocket,
    // last_sequence: AtomicUsize
}

impl NetlinkSocket {
    pub fn create() -> Result<Self> {
        let fd = unsafe {
            socket(
                AddressFamily::AF_NETLINK,
                sys::SocketType::SOCK_DGRAM,
                sys::SocketFlags::SOCK_CLOEXEC,
                sys::SocketProtocol::NETLINK_ROUTE,
            )?
        };

        // Bind to pid=0 (which will casue the kernel to auto-assign us a unique pid
        // identifying this socket).
        unsafe { sys::bind(&fd, &sys::SocketAddr::netlink(0, 0))? };

        Ok(Self { inner: MessageSocket::new(fd) })
    }

    // TODO: consider making more of these functions require '&mut self'. We can
    // probably allow concurrent sends, but receives will be de-multiplexes based on
    // sequence number.

    /// Sends a message using the 'nlmsghdr' format.
    pub async fn send_to_kernel(&self, message: &mut [u8]) -> Result<()> {
        let message_len = message.len();
        let (message_header, _) = parse_cstruct_mut::<nlmsghdr>(message)?;
        message_header.nlmsg_len = message_len as u32;

        let kernel_addr = sys::SocketAddr::netlink(0, 0);

        // TODO: Check the return value.
        self.inner.send_to(message, &kernel_addr).await?;

        Ok(())
    }

    // TODO: Verify that the response sequence matches the request sequence.

    pub async fn recv_message(&self) -> Result<(nlmsghdr, Vec<u8>)> {
        let mut buf = [0u8; 8192];
        let n = self.inner.recv(&mut buf).await?;

        let ((message_header, mut message_payload), rest) =
            parse_cstruct_with_payload::<nlmsghdr>(&buf[0..n])?;
        if !rest.is_empty() {
            return Err(err_msg("Extra data after message"));
        }

        Ok((message_header.clone(), message_payload.to_vec()))
    }

    pub async fn recv_ack(&self) -> Result<()> {
        let (hdr, payload) = self.recv_message().await?;

        if hdr.nlmsg_type != libc::NLMSG_ERROR as u16 {
            return Err(err_msg("Expected to receive error message"))
        }

        let (e, _) = parse_cstruct::<nlmsgerr>(&payload)?;

        // TODO: Also correlate the e.msg sequence with the original request we sent.

        if e.error != 0 {
            return Err(format_err!("Non zero error: {}", e.error));
        }

        Ok(())
    }

    /// Receives messages which use the 'nlmsghdr' format.
    /// (this is for multipart messages requested with NLM_F_DUMP)
    pub fn recv_messages(&self) -> NetlinkMessageReceiver {
        NetlinkMessageReceiver {
            socket: self,
            buffer: [0u8; 8192],
            buffer_offset: 0,
            buffer_length: 0,
            received_all_parts: false,
        }
    }
}

pub struct NetlinkMessageReceiver<'a> {
    socket: &'a NetlinkSocket,

    buffer: [u8; 8192],
    buffer_offset: usize,
    buffer_length: usize,

    /// NOTE: We currently assume that we are only receiving messages for a
    /// single sequence at a time.
    received_all_parts: bool,
}

impl<'a> NetlinkMessageReceiver<'a> {
    pub async fn next<'b>(&'b mut self) -> Result<Option<(&'b nlmsghdr, &'b [u8])>> {
        if self.buffer_offset == self.buffer_length {
            if self.received_all_parts {
                return Ok(None);
            }

            let n = self.socket.inner.recv(&mut self.buffer).await?;
            self.buffer_offset = 0;
            self.buffer_length = n;
        }

        let input = &self.buffer[self.buffer_offset..self.buffer_length];

        let ((message_header, mut message_payload), rest) =
            parse_cstruct_with_payload::<nlmsghdr>(input)?;

        self.buffer_offset += input.len() - rest.len();

        // TODO: Also check that the multi-part flag is set (if not, assert there is no
        // more data in the current buffer).

        // TODO: Check the sequence.

        // let is_multi_part =

        if message_header.nlmsg_type == libc::NLMSG_DONE as u16 {
            self.received_all_parts = true;
            if self.buffer_offset != self.buffer_length {
                return Err(err_msg("Extra data after final message"));
            }

            self.buffer_offset = 0;
            self.buffer_length = 0;
            return Ok(None);
        }

        if message_header.nlmsg_type == libc::NLMSG_ERROR as u16 {
            return Err(err_msg("Received error"));
        }

        Ok(Some((message_header, message_payload)))
    }
}

#[derive(Default, Debug)]
pub struct Interface {
    pub index: u32,
    pub name: String,
    pub loopback: bool,
    pub up: bool,
    pub operational_state: OperationalState,
    pub link_address: Vec<u8>,
    pub link_broadcast_address: Vec<u8>,
    pub addrs: Vec<InterfaceAddr>,
    pub virt: bool,
}

enum_def!(OperationalState u8 =>
    Unknown = 0,
    NotPresent = 1,
    Down = 2,
    LowerLayerDown = 3,
    Testing = 4,
    Dormant = 5,
    Up = 6
);

impl Default for OperationalState {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug)]
pub struct InterfaceAddr {
    pub family: InterfaceAddrFamily,
    pub address: Vec<u8>,
    pub local_address: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub enum InterfaceAddrFamily {
    INET,
    INET6,
}

pub async fn add_interface_address(
    interface_index: usize,
) -> Result<()> {
    let sock = NetlinkSocket::create()?;

    let mut req = vec![];
    serialize_cstruct(
        &nlmsghdr {
            nlmsg_len: 0,
            nlmsg_type: libc::RTM_NEWADDR,
            nlmsg_flags: (libc::NLM_F_REQUEST | libc::NLM_F_ACK) as u16,
            nlmsg_seq: 1,
            nlmsg_pid: 0,
        },
        &mut req,
    );
    serialize_cstruct(
        &ifaddrmsg {
            ifa_family: libc::AF_INET as u8,
            ifa_prefixlen: 16, // 255.255.0.0,
            ifa_flags: libc::IFA_F_PERMANENT as u8, // Not dynamic in DHCP
            ifa_scope: libc::RT_SCOPE_UNIVERSE,
            ifa_index: interface_index as u32,
        },
        &mut req,
    );

    serialize_cstruct_with_payload(
        rtattr {
            rta_len: 0, // Filled in later
            rta_type: libc::IFA_ADDRESS
        },
        &[
            10,
            4,
            0,
            1
        ],
        &mut req
    );

    serialize_cstruct_with_payload(
        rtattr {
            rta_len: 0, // Filled in later
            rta_type: libc::IFA_LOCAL
        },
        &[
            10,
            4,
            0,
            1
        ],
        &mut req
    );

    sock.send_to_kernel(&mut req).await?;

    sock.recv_ack().await?;

    Ok(())
}

pub async fn up_interface(
    interface_index: usize
) -> Result<()> {
    let sock = NetlinkSocket::create()?;

    let mut req = vec![];
    serialize_cstruct(
        &nlmsghdr {
            nlmsg_len: 0,
            nlmsg_type: libc::RTM_SETLINK,
            nlmsg_flags: (libc::NLM_F_REQUEST | libc::NLM_F_ACK) as u16,
            nlmsg_seq: 1,
            nlmsg_pid: 0,
        },
        &mut req,
    );
    serialize_cstruct(
        &ifinfomsg {
            ifi_family: 0, // Unused
            ifi_type: 0, // Unused
            ifi_index: interface_index as i32,
            ifi_flags: libc::IFF_UP as u32,
            ifi_change: libc::IFF_UP as u32,
        },
        &mut req,
    );

    sock.send_to_kernel(&mut req).await?;

    sock.recv_ack().await?;

    Ok(())
}

pub async fn read_interfaces() -> Result<Vec<Interface>> {
    let sock = NetlinkSocket::create()?;

    // TODO: Automate the sequence stuff.

    /*
    pub const IFLA_UNSPEC: ::c_ushort = 0;
    pub const IFLA_ADDRESS: ::c_ushort = 1;
    pub const IFLA_BROADCAST: ::c_ushort = 2;
    pub const IFLA_IFNAME: ::c_ushort = 3;
    pub const IFLA_MTU: ::c_ushort = 4;
    pub const IFLA_LINK: ::c_ushort = 5;
    pub const IFLA_QDISC: ::c_ushort = 6;
    pub const IFLA_STATS: ::c_ushort = 7;
    pub const IFLA_COST: ::c_ushort = 8;
    pub const IFLA_PRIORITY: ::c_ushort = 9;
    pub const IFLA_MASTER: ::c_ushort = 10;
    */

    // TODO: This code may not behave well if an interface is added or removed
    // between requests.

    // Mapping from interface index to the currently constructed interface.
    let mut interfaces: HashMap<usize, Interface> = HashMap::new();

    let mut link_request = vec![];
    serialize_cstruct(
        &nlmsghdr {
            nlmsg_len: 0,
            nlmsg_type: libc::RTM_GETLINK,
            nlmsg_flags: (libc::NLM_F_DUMP | libc::NLM_F_REQUEST) as u16,
            nlmsg_seq: 1,
            nlmsg_pid: 0,
        },
        &mut link_request,
    );
    serialize_cstruct(&ifinfomsg::default(), &mut link_request);
    sock.send_to_kernel(&mut link_request).await?;

    // println!("My PID is {}", unsafe { sys::getpid() });
    // println!("Header length: {}", std::mem::size_of::<nlmsghdr>());
    // println!("Info size: {}", std::mem::size_of::<ifinfomsg>());
    // println!("Attr size: {}", std::mem::size_of::<rtattr>());

    let mut message_receiver = sock.recv_messages();

    while let Some((message_header, mut message_payload)) = message_receiver.next().await? {
        // println!("{:?}", message_header);

        let info: &ifinfomsg = parse_next!(message_payload, parse_cstruct);
        // println!("{:?}", info);

        let iface = interfaces.entry(info.ifi_index as usize).or_default();
        iface.index = info.ifi_index as u32;

        iface.up = info.ifi_flags & (libc::IFF_UP as u32) != 0;
        iface.loopback = info.ifi_flags & (libc::IFF_LOOPBACK as u32) != 0;

        while !message_payload.is_empty() {
            let (attr, value): (&rtattr, _) =
                parse_next!(message_payload, parse_cstruct_with_payload);

            if attr.rta_type == libc::IFLA_IFNAME {
                iface.name = read_null_terminated_string(value)?;
            }
            else if attr.rta_type == libc::IFLA_ADDRESS {
                iface.link_address = value.to_vec();
            }
            else if attr.rta_type == libc::IFLA_BROADCAST {
                iface.link_broadcast_address = value.to_vec();
            }
            else if attr.rta_type == libc::IFLA_OPERSTATE {
                if value.len() != 1 {
                    return Err(err_msg("Invalid operstate value length"));
                }

                iface.operational_state = OperationalState::from_value(value[0])?;
            }
            else if attr.rta_type == libc::IFLA_LINKINFO {
                iface.virt = true;
            }
            else {

            }
        }
    }

    //////////////////////

    let mut addr_request = vec![];
    serialize_cstruct(
        &nlmsghdr {
            nlmsg_len: 0,
            nlmsg_type: libc::RTM_GETADDR,
            nlmsg_flags: (libc::NLM_F_DUMP | libc::NLM_F_REQUEST) as u16,
            nlmsg_seq: 2,
            nlmsg_pid: 0,
        },
        &mut addr_request,
    );
    serialize_cstruct(&ifaddrmsg::default(), &mut addr_request);
    sock.send_to_kernel(&mut addr_request).await?;

    let mut message_receiver = sock.recv_messages();

    while let Some((message_header, mut message_payload)) = message_receiver.next().await? {
        // println!("{:?}", message_header);

        let info: &ifaddrmsg = parse_next!(message_payload, parse_cstruct);

        let mut addr = InterfaceAddr {
            family: match info.ifa_family as i32 {
                libc::AF_INET => InterfaceAddrFamily::INET,
                libc::AF_INET6 => InterfaceAddrFamily::INET6,
                _ => continue,
            },
            address: vec![],
            local_address: vec![],
        };

        while !message_payload.is_empty() {
            let (attr, value): (&rtattr, _) =
                parse_next!(message_payload, parse_cstruct_with_payload);

            if attr.rta_type == libc::IFA_ADDRESS {
                addr.address = value.to_vec();
            }
            if attr.rta_type == libc::IFA_LOCAL {
                addr.local_address = value.to_vec();
            }

            // println!("== {:?}", attr);
            // println!("== {:?}", common::bytes::Bytes::from(value));
        }

        if let Some(iface) = interfaces.get_mut(&(info.ifa_index as usize)) {
            iface.addrs.push(addr);
        }
    }

    Ok(interfaces.into_values().collect())
}

#[derive(Debug)]
pub struct Route {
    pub family: InterfaceAddrFamily,
    pub typ: RouteType,
    pub table: RouteTable,
    pub scope: RouteScope,
    pub source: Option<Vec<u8>>,
    pub preferred_source: Option<Vec<u8>>,
    pub destination: Option<Vec<u8>>,
    pub gateway: Option<Vec<u8>>,
    pub priority: Option<u32>,
    pub metrics: Option<u32>,
    pub input_interface_index: Option<u32>,
    pub output_interface_index: Option<u32>,
}

enum_def_with_unknown!(RouteType u8 =>
    Unicast = libc::RTN_UNICAST,
    Local = libc::RTN_LOCAL,
    Broadcast = libc::RTN_BROADCAST,
    Anycast = libc::RTN_ANYCAST,
    Multicast = libc::RTN_MULTICAST,
    Blackhole = libc::RTN_BLACKHOLE,
    Unreachable = libc::RTN_UNREACHABLE,
    Prohibit = libc::RTN_PROHIBIT,
    Throw = libc::RTN_THROW,
    NAT = libc::RTN_NAT
);

enum_def_with_unknown!(RouteScope u8 =>
    Universe = libc::RT_SCOPE_UNIVERSE,
    Site = libc::RT_SCOPE_SITE,
    Link = libc::RT_SCOPE_LINK,
    Host = libc::RT_SCOPE_HOST,
    Nowhere = libc::RT_SCOPE_NOWHERE
);

enum_def_with_unknown!(RouteTable u8 =>
    Default = libc::RT_TABLE_DEFAULT,
    Main = libc::RT_TABLE_MAIN,
    Local = libc::RT_TABLE_LOCAL
);


pub async fn read_routes() -> Result<Vec<Route>> {
    let sock = NetlinkSocket::create()?;

    let mut link_request = vec![];
    serialize_cstruct(
        &nlmsghdr {
            nlmsg_len: 0,
            nlmsg_type: libc::RTM_GETROUTE,
            nlmsg_flags: (libc::NLM_F_DUMP | libc::NLM_F_REQUEST) as u16,
            nlmsg_seq: 1,
            nlmsg_pid: 0,
        },
        &mut link_request,
    );
    serialize_cstruct(&ifinfomsg::default(), &mut link_request);
    sock.send_to_kernel(&mut link_request).await?;

    let mut message_receiver = sock.recv_messages();

    let mut out = vec![];

    while let Some((message_header, mut message_payload)) = message_receiver.next().await? {
        let info: &rtmsg = parse_next!(message_payload, parse_cstruct);

        let family = match info.rtm_family as i32 {
            libc::AF_INET => InterfaceAddrFamily::INET,
            libc::AF_INET6 => InterfaceAddrFamily::INET6,
            _ => {
                // println!("UNKNOWN ADDR FAMILY");
                continue
            },
        };

        let typ = RouteType::from_value(info.rtm_type);
        let scope = RouteScope::from_value(info.rtm_scope);

        let mut route = Route {
            family,
            typ,
            scope,
            table: RouteTable::from_value(info.rtm_table),
            source: None,
            preferred_source: None,
            destination: None,
            gateway: None,
            metrics: None,
            priority: None,
            input_interface_index: None,
            output_interface_index: None,
        };

        while !message_payload.is_empty() {
            let (attr, value): (&rtattr, _) =
                parse_next!(message_payload, parse_cstruct_with_payload);

            if attr.rta_type == libc::RTA_DST {
                route.destination = Some(value.to_vec());
            } else if attr.rta_type == libc::RTA_SRC {
                route.source = Some(value.to_vec());
            } else if attr.rta_type == libc::RTA_PREFSRC {
                route.preferred_source = Some(value.to_vec());
            } else if attr.rta_type == libc::RTA_GATEWAY {
                route.gateway = Some(value.to_vec());
            } else if attr.rta_type == libc::RTA_PRIORITY {
                if value.len() != 4 {
                    return Err(err_msg("Invalid u32"));
                }

                route.priority = Some(u32::from_le_bytes(*array_ref![value, 0, 4]));
            } else if attr.rta_type == libc::RTA_METRICS {
                if value.len() != 4 {
                    return Err(err_msg("Invalid u32"));
                }

                route.metrics = Some(u32::from_le_bytes(*array_ref![value, 0, 4]));
            } else if attr.rta_type == libc::RTA_IIF {
                if value.len() != 4 {
                    return Err(err_msg("Invalid u32"));
                }

                route.input_interface_index = Some(u32::from_le_bytes(*array_ref![value, 0, 4]));
            } else if attr.rta_type == libc::RTA_OIF {
                if value.len() != 4 {
                    return Err(err_msg("Invalid u32"));
                }

                route.output_interface_index = Some(u32::from_le_bytes(*array_ref![value, 0, 4]));
            } else if attr.rta_type == libc::RTA_TABLE {
                // TODO
            } else if attr.rta_type == libc::RTA_PREF {
                // TODO
            } else if attr.rta_type == libc::RTA_CACHEINFO {
                // TODO:
            }
            else {
                // println!("Unknown attr: {:?}", attr);
            }
        }

        out.push(route);
    }

    Ok(out)
}

/// Tries to find the local network ip address of the current machine.
///
/// Basically we to pick the IP address associated with the interface used by the default
/// global route on the machine.
///
/// If both a V4 and V6 address are available, we will prefer the V4 address (as
/// it is likely shorter and more user friendly).
pub async fn local_ip() -> Result<IPAddress> {
    let mut routes = read_routes().await?.into_iter()
        .filter(|route| {
            route.scope == RouteScope::Universe &&
            route.typ == RouteType::Unicast &&
            route.output_interface_index.is_some()
        })
        .collect::<Vec<_>>();

    routes.sort_by_key(|route| route.priority.unwrap_or_default());

    if routes.is_empty() {
        return Err(err_msg("Unable to find any default routes"));
    }

    let iface_index = routes[0].output_interface_index.unwrap();

    let ifaces = read_interfaces().await?;

    let mut found_ip = None;
    for iface in ifaces {
        if !iface.up || iface.loopback || iface.operational_state != OperationalState::Up {
            continue;
        }

        if iface.index != iface_index {
            continue;
        }

        let mut found_v4 = false;

        for addr in iface.addrs {
            match addr.family {
                InterfaceAddrFamily::INET => {
                    found_v4 = true;
                    found_ip = Some(IPAddress::V4(*array_ref![addr.address, 0, 4]));
                }
                InterfaceAddrFamily::INET6 => {
                    if found_v4 {
                        continue;
                    }

                    found_ip = Some(IPAddress::V6(*array_ref![addr.address, 0, 16]));
                }
            }
        }
    }

    found_ip.ok_or_else(|| err_msg("No suitable local ips found"))
}

// TODO: Dedup me
fn parse_cstruct<T>(input: &[u8]) -> Result<(&T, &[u8])> {
    let size = std::mem::size_of::<T>();
    let (data, rest) = parse_payload(input, size)?;

    Ok((unsafe { std::mem::transmute(data.as_ptr()) }, rest))
}

// TODO: Dedup me
fn parse_cstruct_mut<T>(input: &mut [u8]) -> Result<(&mut T, &[u8])> {
    let size = std::mem::size_of::<T>();
    let (data, rest) = parse_payload_mut(input, size)?;

    Ok((unsafe { std::mem::transmute(data.as_mut_ptr()) }, rest))
}

fn parse_cstruct_with_payload<T: StructLength>(input: &[u8]) -> Result<((&T, &[u8]), &[u8])> {
    let (value, rest) = parse_cstruct::<T>(input)?;

    // NOTE: This should never overflow as we can't consume a negative number of
    // bytes.
    let input_consumed = input.len() - rest.len();

    if input_consumed > value.struct_length() {
        return Err(err_msg("Overflow struct payload"));
    }

    let payload_len = value.struct_length() - input_consumed;

    let (payload, rest2) = parse_payload(rest, payload_len)?;

    Ok(((value, payload), rest2))
}

fn serialize_cstruct_with_payload<T: StructLength>(mut value: T, payload: &[u8], out: &mut Vec<u8>) {
    value.set_struct_length(std::mem::size_of::<T>() + payload.len());
    serialize_cstruct(&value, out);
    out.extend_from_slice(payload);
}

fn parse_payload(input: &[u8], length: usize) -> Result<(&[u8], &[u8])> {
    let length_aligned = length + common::block_size_remainder(4, length as u64) as usize;
    if input.len() < length_aligned {
        return Err(format_err!("Not enough bytes. Length: {}", length));
    }

    Ok((&input[0..length], &input[length_aligned..]))
}

fn parse_payload_mut(input: &mut [u8], length: usize) -> Result<(&mut [u8], &[u8])> {
    let length_aligned = length + common::block_size_remainder(4, length as u64) as usize;
    if input.len() < length_aligned {
        return Err(format_err!("Not enough bytes. Length: {}", length));
    }

    let (a, b) = input.split_at_mut(length);

    Ok((a, &b[(length_aligned - length)..]))
}

// TODO: Dedup me
fn serialize_cstruct<T>(value: &T, out: &mut Vec<u8>) {
    let data: &[u8] =
        unsafe { std::slice::from_raw_parts(std::mem::transmute(value), std::mem::size_of::<T>()) };
    out.extend_from_slice(data);

    let mut len = data.len();

    while len % 4 != 0 {
        out.push(0);
        len += 1;
    }
}

trait StructLength {
    fn struct_length(&self) -> usize;

    fn set_struct_length(&mut self, len: usize);
}


// TODO: Move these to third_party

#[repr(C)]
#[derive(Default, Debug)]
struct nlmsgerr {
    error: sys::c_int,
    msg: nlmsghdr,
}

#[repr(C)]
#[derive(Default, Debug, Clone)]
struct nlmsghdr {
    nlmsg_len: u32,   /* Length of message including header */
    nlmsg_type: u16,  /* Type of message content */
    nlmsg_flags: u16, /* Additional flags */
    nlmsg_seq: u32,   /* Sequence number */
    nlmsg_pid: u32,   /* Sender port ID */
}

impl StructLength for nlmsghdr {
    fn struct_length(&self) -> usize {
        self.nlmsg_len as usize
    }

    fn set_struct_length(&mut self, len: usize) {
        self.nlmsg_len = len as u32; 
    }
}

#[repr(C)]
#[derive(Default, Debug)]
struct ifinfomsg {
    ifi_family: sys::c_uchar, /* AF_UNSPEC */
    ifi_type: sys::c_ushort,  /* Device type */
    ifi_index: sys::c_int,    /* Interface index */
    ifi_flags: sys::c_uint,   /* Device flags */
    ifi_change: sys::c_uint,  /* change mask */
}


// These are also aligned to 4 bytes (both the start of the data and the end of
// the data)

/// This is followed by the value of the
#[repr(C)]
#[derive(Debug)]
struct rtattr {
    rta_len: sys::c_ushort,  /* Length of option */
    rta_type: sys::c_ushort, /* Type of option */
}

impl StructLength for rtattr {
    fn struct_length(&self) -> usize {
        self.rta_len as usize
    }

    fn set_struct_length(&mut self, len: usize) {
        self.rta_len = len as sys::c_ushort; 
    }    
}

#[repr(C)]
#[derive(Debug, Default)]
struct ifaddrmsg {
    ifa_family: sys::c_uchar,    /* Address type */
    ifa_prefixlen: sys::c_uchar, /* Prefixlength of address */
    ifa_flags: sys::c_uchar,     /* Address flags */
    ifa_scope: sys::c_uchar,     /* Address scope */
    ifa_index: sys::c_uint,      /* Interface index */
}


#[repr(C)]
#[derive(Debug, Default)]
struct rtmsg {
    rtm_family: sys::c_uchar,   /* Address family of route */
    rtm_dst_len: sys::c_uchar,  /* Length of destination */
    rtm_src_len: sys::c_uchar,  /* Length of source */
    rtm_tos: sys::c_uchar,      /* TOS filter */
    rtm_table: sys::c_uchar,    /* Routing table ID;
                                   see RTA_TABLE below */
    rtm_protocol: sys::c_uchar, /* Routing protocol; see below */
    rtm_scope: sys::c_uchar,    /* See below */
    rtm_type: sys::c_uchar,     /* See below */
    rtm_flags: sys::c_uint,
}
