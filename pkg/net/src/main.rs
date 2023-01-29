extern crate common;
extern crate net;
#[macro_use]
extern crate macros;

use std::string::ToString;
use std::time::Duration;

use common::errors::*;
use common::io::{Readable, Writeable};
use net::dns;
use net::tcp::{TcpListener, TcpStream};

async fn server() -> Result<()> {
    let mut listener = TcpListener::bind("0.0.0.0:8000".parse()?).await?;

    // TCP_NODELAY

    // TODO: Add SO_REUSE

    loop {
        let mut stream = listener.accept().await?;
        stream.set_nodelay(true)?;

        println!("Got 1");

        let mut request = [0u8; 1024];
        let n = stream.read(&mut request).await?;

        println!("Did read: {}", n);

        stream
            .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
            .await?;

        // executor::timeout(Duration::from_secs(1)).await;
    }

    println!("Done!");

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let ip = net::netlink::local_ip()?;
    println!("My local ip: {:?}", ip.to_string());

    let ifaces = net::netlink::read_interfaces()?;
    println!("{:#?}", ifaces);

    return Ok(());

    let mut client = dns::Client::create_insecure().await?;

    // TODO: Auto-add the '.'
    let ip = client.resolve_addr("google.com.").await?;

    println!("{:?}", ip);

    /*
    let mut query_builder = QueryBuilder::new(1);

    query_builder.add_question(
        "_imaps._tcp.gmail.com.".try_into()?,
        RecordType::SRV,
        Class::IN,
    );
    // query_builder.add_question("google.com.".try_into()?, RecordType::A,
    // Class::IN);

    // query_builder.add_question("www.microsoft.com.".try_into()?, RecordType::A,
    // Class::IN);

    // TODO: Verify that this is at most 512 bytes
    let query_data = query_builder.build();

    println!("{:?}", Message::parse_complete(&query_data)?);



    socket.send(&query_data).await?;

    for i in 0..2 {
        let mut response = vec![0u8; 512];
        let n = socket.recv(&mut response).await?;

        let reply = Message::parse_complete(&response[0..n])?;

        println!("{:?}", reply);
    }
    */

    Ok(())
}
