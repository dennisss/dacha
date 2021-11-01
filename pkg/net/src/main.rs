extern crate common;
extern crate net;

use common::errors::*;
use net::dns;

// Port 53

async fn run() -> Result<()> {
    let ip = net::netlink::local_ip()?;
    println!("My local ip: {:?}", ip.to_string());

    // let ifaces = net::netlink::read_interfaces().await?;
    // println!("{:#?}", ifaces);

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

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
