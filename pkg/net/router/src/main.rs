
#[macro_use]
extern crate macros;

use common::errors::*;

#[executor_main]
async fn main() -> Result<()> {

    // AF_INET, SOCK_RAW, IPPROTO_IP

    /*
    socket(AF_PACKET, SOCK_RAW, htons(ETH_P_IP))
    - Will receive raw ethernet packets
    - Must assemble the ethernet packets
    - 

    */

    let fd = unsafe {
        sys::socket(
            sys::AddressFamily::AF_INET,
            sys::SocketType::SOCK_RAW,
            sys::SocketFlags::SOCK_CLOEXEC,
            sys::SocketProtocol::IP, // IPPROTO_IP
        )?
    };

    let socket = net::udp::MessageSocket::new(fd);

    let mut buf = vec![0u8; 1024 * 64];
    loop {
        let (n, addr) = socket.recv_from(&mut buf).await?;

        println!("{}, {:?}", n, addr);



    }



    Ok(())
}