#![no_std]

#[macro_use]
extern crate core;

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate std;

#[macro_use]
extern crate common;
#[macro_use]
extern crate parsing;
extern crate automata;
#[macro_use]
extern crate regexp_macros;
extern crate crypto;
extern crate executor;
extern crate libc;
extern crate nix;
extern crate radix;
extern crate sys;
#[macro_use]
extern crate macros;

pub mod backoff;
pub mod dns;
mod endian;
pub mod error;
pub mod ip;
mod ip_syntax;
pub mod netlink;
pub mod tcp;
pub mod udp;
mod utils;

pub use ip_syntax::parse_port;
pub use netlink::local_ip;

#[cfg(test)]
mod tests {
    use crate::error::NetworkError;

    use super::*;

    use alloc::vec::Vec;
    use std::time::Duration;

    use common::{
        errors::*,
        io::{IoError, IoErrorKind, Readable, Writeable},
    };

    #[test]
    fn get_local_ip_test() {
        local_ip().unwrap();
    }

    #[testcase]
    async fn tcp_client_server_test() -> Result<()> {
        let addr: ip::SocketAddr = "127.0.0.1:8123".parse()?;

        let server_listener = tcp::TcpListener::bind(addr.clone()).await.unwrap();

        async fn server_fn(mut server_listener: tcp::TcpListener) -> Result<Vec<u8>> {
            let mut server_stream = server_listener.accept().await?;

            let mut buf = vec![0u8; 4];
            server_stream.read_exact(&mut buf).await?;

            server_stream.write_all(&[5, 6, 7, 8]).await?;

            Ok(buf)
        }

        let server = executor::spawn(server_fn(server_listener));

        let mut client = tcp::TcpStream::connect(addr.clone()).await.unwrap();
        client.write_all(&[1, 2, 3, 4]).await?;

        let mut client_buf = vec![0u8; 4];
        client.read_exact(&mut client_buf).await?;

        let server_buf = server.join().await?;

        assert_eq!(&server_buf, &[1, 2, 3, 4]);
        assert_eq!(&client_buf, &[5, 6, 7, 8]);

        Ok(())
    }

    #[testcase]
    async fn dns_regular_client() -> Result<()> {
        let mut client = dns::Client::create_insecure().await?;

        /*
        TODO: Why can't I query 'lem.ma.'
        */

        assert_eq!(
            client.resolve_addr("google.com.").await?,
            ip::IPAddress::V4([35, 241, 17, 240])
        );

        Ok(())
    }

    // TODO: Also test the multi-cast client?

    // TODO: SO_LINGER

    #[testcase]
    async fn tcp_failure_modes() -> Result<()> {
        let mut buffer = vec![0u8; 256];

        let addr: ip::SocketAddr = "127.0.0.1:8124".parse()?;

        let mut server_listener = tcp::TcpListener::bind(addr.clone()).await?;

        {
            let mut client_stream = tcp::TcpStream::connect(addr.clone()).await?;
            let mut server_stream = server_listener.accept().await?;

            drop(server_stream);

            // Wait for TCP close packets to propagate.
            executor::sleep(Duration::from_millis(10)).await?;

            // Server stream is completely closed so we should see the end of the stream.
            assert_eq!(client_stream.read(&mut buffer).await?, 0);
            assert_eq!(client_stream.read(&mut buffer).await?, 0);

            // The client should notice the server reader was closed eventually.
            let mut saw_closed = false;
            for _ in 0..10 {
                match client_stream.write(&buffer).await {
                    Ok(_) => {}
                    Err(e) => {
                        if let Some(IoError {
                            kind: IoErrorKind::RemoteReaderClosed,
                            ..
                        }) = e.downcast_ref()
                        {
                            saw_closed = true;
                            break;
                        }

                        return Err(e);
                    }
                }
            }
            assert!(saw_closed);
        }

        // Only dropping server writer.
        {
            let mut client_stream = tcp::TcpStream::connect(addr.clone()).await?;
            let mut server_stream = server_listener.accept().await?;

            let (server_reader, server_writer) = server_stream.split();
            drop(server_writer);

            assert_eq!(client_stream.read(&mut buffer).await?, 0);

            drop(server_reader);
        }

        /*
        // Writing way too much.
        {
            let mut client_stream = tcp::TcpStream::connect(addr.clone()).await?;
            let mut server_stream = server_listener.accept().await?;

            for i in 0..(10_000_000 / 256) {
                client_stream.write_all(&buffer).await?;
            }
        }
        */

        Ok(())
    }

    /*
    Potential network failure modes:
    - Reading and

    Too much bytes sent to the server

    */
}
