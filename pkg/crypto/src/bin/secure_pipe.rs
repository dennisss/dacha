#[macro_use]
extern crate common;
extern crate crypto;
#[macro_use]
extern crate macros;

use common::async_std::fs;
use common::async_std::net::TcpStream;
use common::async_std::task;
use common::bytes::Bytes;
use common::errors::*;
use common::io::{Readable, Writeable};

#[derive(Args)]
struct Args {
    command: Command,
}

// #[arg(positional)]

#[derive(Args)]
enum Command {
    #[arg(name = "client")]
    Client(ClientCommand),

    #[arg(name = "server")]
    Server(ServerCommand),
}

#[derive(Args)]
struct ClientCommand {
    /// e.g. "google.com:443"
    addr: String,
}

#[derive(Args)]
struct ServerCommand {
    port: u16,
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::Client(cmd) => {
            let raw_stream = TcpStream::connect(&cmd.addr).await?;
            let reader = Box::new(raw_stream.clone());
            let writer = Box::new(raw_stream);

            let mut client_options = crypto::tls::options::ClientOptions::recommended();
            client_options.hostname = "localhost".into();
            client_options.alpn_ids.push("h2".into());
            client_options.alpn_ids.push("http/1.1".into());
            client_options.trust_server_certificate = true;

            let mut client = crypto::tls::client::Client::new();
            let mut stream = client.connect(reader, writer, &client_options).await?;

            let mut buf = vec![0u8; 100];
            let n = stream.reader.read(&mut buf).await?;

            println!("Read {}: {:?}", n, Bytes::from(&buf[0..n]));

            /*

            stream
                .writer
                .write_all(b"GET / HTTP/1.1\r\nHost: google.com\r\n\r\n")
                .await?;

            let mut buf = vec![];
            buf.resize(100, 0);
            stream.reader.read_exact(&mut buf).await?;
            println!("{}", String::from_utf8(buf).unwrap());
            */

            Ok(())
        }
        Command::Server(cmd) => {
            let certificate_file =
                fs::read(project_path!("testdata/certificates/server-ec.crt")).await?;
            let private_key_file =
                fs::read(project_path!("testdata/certificates/server-ec.key")).await?;

            let options = crypto::tls::options::ServerOptions::recommended(
                certificate_file.into(),
                private_key_file.into(),
            )?;
            crypto::tls::server::Server::run(cmd.port, &options).await
        }
    }
}

fn main() -> Result<()> {
    task::block_on(run())
}
