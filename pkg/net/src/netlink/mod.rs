use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::FromRawFd;
use std::sync::atomic::AtomicUsize;

use common::async_std::net::UdpSocket;
use common::errors::*;
use nix::sys::socket::recvmsg;
use nix::sys::socket::sendmsg;
use nix::sys::socket::sockopt::ReusePort;
use nix::sys::socket::MsgFlags;
use nix::sys::socket::{
    AddressFamily, InetAddr, NetlinkAddr, SockAddr, SockFlag, SockProtocol, SockType,
};
use nix::sys::uio::IoVec;

use crate::ip::IPAddress;

/*

Must use these macros to access things:
https://man7.org/linux/man-pages/man3/netlink.3.html

NETLINK_ROUTE
    https://man7.org/linux/man-pages/man7/rtnetlink.7.html


*/

struct NetlinkSocket {
    fd: i32,
    // last_sequence: AtomicUsize
}

impl Drop for NetlinkSocket {
    fn drop(&mut self) {
        let _ = unsafe { libc::close(self.fd) };
    }
}

impl NetlinkSocket {
    pub fn create() -> Result<Self> {
        // Wrap the fd as soon as possible to ensure that it is closed on errors via the
        // drop implementation.
        let inst = {
            let fd = nix::sys::socket::socket(
                AddressFamily::Netlink,
                SockType::Datagram,
                SockFlag::SOCK_CLOEXEC,
                SockProtocol::NetlinkRoute,
            )?;

            Self { fd }
        };

        // Bind to pid=0 (which will casue the kernel to auto-assign us a unique pid
        // identifying this socket).
        nix::sys::socket::bind(inst.fd, &SockAddr::Netlink(NetlinkAddr::new(0, 0)))?;

        Ok(inst)
    }

    // TODO: consider making more of these functions require '&mut self'. We can
    // probably allow concurrent sends, but receives will be de-multiplexes based on
    // sequence number.

    pub fn send_to_kernel(&self, message: &mut [u8]) -> Result<()> {
        let message_len = message.len();
        let (message_header, _) = parse_cstruct_mut::<nlmsghdr>(message)?;
        message_header.nlmsg_len = message_len as u32;

        let kernel_addr = SockAddr::new_netlink(0, 0);

        // TODO: Check the return value.
        sendmsg(
            self.fd,
            &[IoVec::from_slice(message)],
            &[],
            MsgFlags::empty(),
            Some(&kernel_addr),
        )?;

        Ok(())
    }

    // TODO: Verify that the response sequence matches the request sequence.

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

struct NetlinkMessageReceiver<'a> {
    socket: &'a NetlinkSocket,

    buffer: [u8; 8192],
    buffer_offset: usize,
    buffer_length: usize,

    /// NOTE: We currently assume that we are only receiving messages for a
    /// single sequence at a time.
    received_all_parts: bool,
}

impl<'a> NetlinkMessageReceiver<'a> {
    pub fn next<'b>(&'b mut self) -> Result<Option<(&'b nlmsghdr, &'b [u8])>> {
        if self.buffer_offset == self.buffer_length {
            if self.received_all_parts {
                return Ok(None);
            }

            let received = recvmsg(
                self.socket.fd,
                &[IoVec::from_mut_slice(&mut self.buffer)],
                None,
                MsgFlags::empty(),
            )?;
            self.buffer_offset = 0;
            self.buffer_length = received.bytes;
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
    pub name: String,
    pub loopback: bool,
    pub up: bool,
    pub operational_state: OperationalState,
    pub link_address: Vec<u8>,
    pub link_broadcast_address: Vec<u8>,
    pub addrs: Vec<InterfaceAddr>,
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

#[derive(Debug)]
pub enum InterfaceAddrFamily {
    INET,
    INET6,
}

pub fn read_interfaces() -> Result<Vec<Interface>> {
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
    sock.send_to_kernel(&mut link_request)?;

    // println!("My PID is {}", unsafe { libc::getpid() });
    // println!("Header length: {}", std::mem::size_of::<nlmsghdr>());
    // println!("Info size: {}", std::mem::size_of::<ifinfomsg>());
    // println!("Attr size: {}", std::mem::size_of::<rtattr>());

    let mut message_receiver = sock.recv_messages();

    while let Some((message_header, mut message_payload)) = message_receiver.next()? {
        // println!("{:?}", message_header);

        let info: &ifinfomsg = parse_next!(message_payload, parse_cstruct);
        // println!("{:?}", info);

        let iface = interfaces.entry(info.ifi_index as usize).or_default();

        iface.up = info.ifi_flags & (libc::IFF_UP as u32) != 0;
        iface.loopback = info.ifi_flags & (libc::IFF_LOOPBACK as u32) != 0;

        while !message_payload.is_empty() {
            let (attr, value): (&rtattr, _) =
                parse_next!(message_payload, parse_cstruct_with_payload);

            if attr.rta_type == libc::IFLA_IFNAME {
                iface.name = read_null_terminated_string(value)?;
            }
            if attr.rta_type == libc::IFLA_ADDRESS {
                iface.link_address = value.to_vec();
            }
            if attr.rta_type == libc::IFLA_BROADCAST {
                iface.link_broadcast_address = value.to_vec();
            }
            if attr.rta_type == libc::IFLA_OPERSTATE {
                if value.len() != 1 {
                    return Err(err_msg("Invalid operstate value length"));
                }

                iface.operational_state = OperationalState::from_value(value[0])?;
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
    sock.send_to_kernel(&mut addr_request)?;

    let mut message_receiver = sock.recv_messages();

    while let Some((message_header, mut message_payload)) = message_receiver.next()? {
        // println!("{:?}", message_header);

        let info: &ifaddrmsg = parse_next!(message_payload, parse_cstruct);
        // println!("{:?}", info);

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

/// Tries to find the local network ip address of the current machine.
///
/// We assume that there is only one active network interface on the machine.
///
/// If both a V4 and V6 address are available, we will prefer the V4 address (as
/// it is likely shorter and more user friendly).
pub fn local_ip() -> Result<IPAddress> {
    let ifaces = read_interfaces()?;

    let mut found_ip = None;
    for iface in ifaces {
        if !iface.up || iface.loopback || iface.operational_state != OperationalState::Up {
            continue;
        }

        if iface.addrs.len() > 0 && found_ip.is_some() {
            return Err(err_msg("Multiple candidate local ips"));
        }

        let mut found_v4 = false;

        for addr in iface.addrs {
            match addr.family {
                InterfaceAddrFamily::INET => {
                    found_v4 = true;
                    found_ip = Some(IPAddress::V4(addr.address));
                }
                InterfaceAddrFamily::INET6 => {
                    if found_v4 {
                        continue;
                    }

                    found_ip = Some(IPAddress::V6(addr.address));
                }
            }
        }
    }

    found_ip.ok_or_else(|| err_msg("No suitable local ips found"))
}

// TODO: Dedup with stream_deck package.
fn read_null_terminated_string(data: &[u8]) -> Result<String> {
    for i in 0..data.len() {
        if data[i] == 0x00 {
            return Ok(std::str::from_utf8(&data[0..i])?.to_string());
        }
    }

    Err(err_msg("Missing null terminator"))
}

fn parse_cstruct<T>(input: &[u8]) -> Result<(&T, &[u8])> {
    let size = std::mem::size_of::<T>();
    let (data, rest) = parse_payload(input, size)?;

    Ok((unsafe { std::mem::transmute(data.as_ptr()) }, rest))
}

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

fn parse_payload(input: &[u8], length: usize) -> Result<(&[u8], &[u8])> {
    let length_aligned = length + common::block_size_remainder(4, length as u64) as usize;
    if input.len() < length_aligned {
        return Err(err_msg("Not enough bytes"));
    }

    Ok((&input[0..length], &input[length_aligned..]))
}

fn parse_payload_mut(input: &mut [u8], length: usize) -> Result<(&mut [u8], &[u8])> {
    let length_aligned = length + common::block_size_remainder(4, length as u64) as usize;
    if input.len() < length_aligned {
        return Err(err_msg("Not enough bytes"));
    }

    let (a, b) = input.split_at_mut(length);

    Ok((a, &b[(length_aligned - length)..]))
}

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
}

#[repr(C)]
#[derive(Default, Debug)]
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
}

#[repr(C)]
#[derive(Default, Debug)]
struct ifinfomsg {
    ifi_family: libc::c_uchar, /* AF_UNSPEC */
    ifi_type: libc::c_ushort,  /* Device type */
    ifi_index: libc::c_int,    /* Interface index */
    ifi_flags: libc::c_uint,   /* Device flags */
    ifi_change: libc::c_uint,  /* change mask */
}

// These are also aligned to 4 bytes (both the start of the data and the end of
// the data)

/// This is followed by the value of the
#[repr(C)]
#[derive(Debug)]
struct rtattr {
    rta_len: libc::c_ushort,  /* Length of option */
    rta_type: libc::c_ushort, /* Type of option */
}

impl StructLength for rtattr {
    fn struct_length(&self) -> usize {
        self.rta_len as usize
    }
}

#[repr(C)]
#[derive(Debug, Default)]
struct ifaddrmsg {
    ifa_family: libc::c_uchar,    /* Address type */
    ifa_prefixlen: libc::c_uchar, /* Prefixlength of address */
    ifa_flags: libc::c_uchar,     /* Address flags */
    ifa_scope: libc::c_uchar,     /* Address scope */
    ifa_index: libc::c_uint,      /* Interface index */
}
