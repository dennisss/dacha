extern crate common;
extern crate net;
#[macro_use]
extern crate macros;

use std::string::ToString;
use std::time::{Duration, Instant};

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::child_task::ChildTask;
use net::dns;
use net::tcp::{TcpListener, TcpStream};

const BLOCK_SIZE: usize = 4 * 4096;

const TARGET_BYTES: usize = 100 * 1024 * 1024;

async fn server(mut listener: TcpListener) -> Result<()> {
    let mut stream = listener.accept().await?;

    let mut total = 0;
    while total < TARGET_BYTES {
        let mut data = vec![0u8; BLOCK_SIZE];
        total += stream.read(&mut data).await?;
    }

    println!("Done!");

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let mut listener = TcpListener::bind("0.0.0.0:8000".parse()?).await?;

    let server = ChildTask::spawn(async move {
        server(listener).await.unwrap();
    });

    let mut client = TcpStream::connect("127.0.0.1:8000".parse()?).await?;

    let start = Instant::now();

    let mut data = vec![0u8; BLOCK_SIZE];
    for i in 0..(TARGET_BYTES / BLOCK_SIZE) {
        client.write_all(&data).await?;
    }

    server.join().await;

    let end = Instant::now();

    println!("Time: {:?}", end - start);

    Ok(())
}
