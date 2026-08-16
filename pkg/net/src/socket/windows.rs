use windows_sys::Win32::Networking::WinSock::{
    bind, closesocket, connect, ioctlsocket, setsockopt, socket, AF_INET, FIONBIO,
    INVALID_SOCKET, IPPROTO_IP, IPPROTO_TCP, IP_MULTICAST_IF, SOCKADDR_IN, SOCKET_ERROR,
    SOCK_STREAM, WSAEWOULDBLOCK, WSAGetLastError, SOCK_DGRAM, IPPROTO_UDP
};

use crate::socket::options::*;
use crate::ip::*;

impl SocketOptions {

    pub fn build(&self) -> std::io::Result<usize> {

        unsafe {
            let (sock_type, sock_proto) = match self.typ.unwrap() {
                SocketType::TCP => (SOCK_STREAM, IPPROTO_TCP),
                SocketType::UDP => (SOCK_DGRAM, IPPROTO_UDP)
            };

            let s = socket(AF_INET as i32, sock_type, sock_proto);
            if s == INVALID_SOCKET {
                return Err(std::io::Error::last_os_error());
            }

            let mut mode: u32 = 1; // 1 = non-blocking, 0 = blocking
            if ioctlsocket(s, FIONBIO, &mut mode) == SOCKET_ERROR {
                closesocket(s);
                return Err(std::io::Error::last_os_error());
            }

            // NOTE: Only makes a difference for UDP.
            if let Some(if_idx) = &self.device_index {
                let if_idx = if_idx.to_be();
                if setsockopt(
                    s,
                    IPPROTO_IP as i32,
                    IP_MULTICAST_IF as i32,
                    core::mem::transmute(&if_idx),
                    std::mem::size_of_val(&if_idx) as i32,
                ) == SOCKET_ERROR
                {
                    closesocket(s);
                    return Err(std::io::Error::last_os_error());
                }
            }

            if let Some(addr) = &self.bind_addr {
                let addr = convert_sockaddr(addr);                
                if bind(
                    s,
                    core::mem::transmute(&addr),
                    std::mem::size_of_val(&addr) as i32,
                ) == SOCKET_ERROR
                {
                    closesocket(s);
                    return Err(std::io::Error::last_os_error());
                }

            }

            if let Some(addr) = &self.connect_addr {
                let addr = convert_sockaddr(addr);

                if connect(
                    s,
                    core::mem::transmute(&addr),
                    std::mem::size_of_val(&addr) as i32,
                ) == SOCKET_ERROR
                {
                    let err = WSAGetLastError();
                    // WSAEWOULDBLOCK is expected; the connection is proceeding asynchronously.
                    if err != WSAEWOULDBLOCK {
                        closesocket(s);
                        return Err(std::io::Error::from_raw_os_error(err));
                    }
                }
            }

            Ok(s)
        }
    }
}

unsafe fn convert_sockaddr(addr: &SocketAddr) -> SOCKADDR_IN {
    let ipv4 = match addr.ip() {
        IPAddress::V4(v) => v,
        _ => todo!()
    };

    let mut out: SOCKADDR_IN = std::mem::zeroed();
    out.sin_family = AF_INET as u16;
    out.sin_addr.S_un.S_addr = u32::from_ne_bytes(*ipv4);
    out.sin_port = addr.port().to_be();

    out
}