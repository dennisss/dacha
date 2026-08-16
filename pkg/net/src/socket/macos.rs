use crate::socket::options::*;

impl SocketOptions {

    pub fn build(&self) -> std::io::Result<i32> {
        unsafe {
            let (sock_type, sock_proto) = match self.typ.unwrap() {
                SocketType::TCP => (libc::SOCK_STREAM, libc::IPPROTO_TCP),
                SocketType::UDP => (libc::SOCK_DGRAM, libc::IPPROTO_UDP)
            };

            // TODO: Adjust family.
            let fd = libc::socket(
                libc::AF_INET, sock_type, sock_proto);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }

            // TODO: ensure fd gets closed on any errors.

            let opt: u32 = 1;
            if libc::setsockopt(
                fd, libc::SOL_SOCKET, libc::SO_NOSIGPIPE,
                core::mem::transmute(&opt), std::mem::size_of::<u32>() as libc::socklen_t
            ) != 0 {
                libc::close(fd);
                return Err(std::io::Error::last_os_error());
            }

            if libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) != 0 {
                libc::close(fd);
                return Err(std::io::Error::last_os_error());
            }

            if let Some(_) = &self.bind_to_device {
                let if_idx = self.device_index.unwrap();

                let opt_res = libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_BOUND_IF,
                    core::mem::transmute(&if_idx),
                    std::mem::size_of::<u32>() as libc::socklen_t,
                );

                if opt_res != 0 {
                    libc::close(fd);
                    return Err(std::io::Error::last_os_error());
                }
            }

            if let Some(bind_addr) = &self.bind_addr {
                let bind_addr = bind_addr.to_libc();

                if libc::bind(
                    fd,
                    &bind_addr,
                    std::mem::size_of_val(&bind_addr) as libc::socklen_t,
                ) < 0
                {
                    libc::close(fd);
                    return Err(std::io::Error::last_os_error());
                }
            }

            if let Some(addr) = &self.connect_addr {
                let connect_addr = addr.to_libc();
                if libc::connect(
                    fd,
                    &connect_addr,
                    std::mem::size_of_val(&connect_addr) as libc::socklen_t,
                ) < 0
                {
                    let err = std::io::Error::last_os_error();
                    // EINPROGRESS means the non-blocking connect has successfully started
                    if err.raw_os_error() != Some(libc::EINPROGRESS) {
                        libc::close(fd);
                        return Err(err);
                    }
                }
            }

            Ok(fd)
        }
    }

    
}