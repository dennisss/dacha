
extern crate tokio;

use super::resp::*;
use tokio::prelude::*;
use tokio::io::copy;
use tokio::net::{TcpListener, TcpStream};

use futures::prelude::*;
use futures::future::*;
use futures::{Stream, Future};
use raft::errors::*;
use std::sync::Arc;


pub trait RedisService {
	fn command(&self, command: RESPCommand) -> RESPObject;
}

fn handle_connection<T: 'static>(sock: TcpStream, inst: Arc<T>) -> impl Future<Item=(), Error=()>
	where T: RedisService + Send + Sync
{

    let framed_sock = tokio::codec::Framed::new(sock, RESPCodec::new());
	let (sink, stream) = framed_sock.split();

	stream
	.map_err(|e| Error::from(e))
	.fold((inst, sink), move |(inst, sink), obj| {
		println!("GOT COMMAND {:?}", obj);

		let cmd = match obj.into_command() {
			Ok(c) => c,
			Err(e) => return Either::A(err(e.into()))
		};

		if cmd[0].as_ref() == "QUIT".as_bytes() {
			return Either::A(err("Closing connection".into()))
		}

		let out = inst.command(cmd);

		Either::B(
			sink
			.send(out)
			.map_err(|e| Error::from(e))
			.and_then(move |sink| {		
				ok((inst, sink))
			})
		)
	})
	.map_err(|e| {
		eprintln!("IO error {:?}", e)
	})
	.map(|_| ())
}

pub fn run_server<T>(inst: Arc<T>) -> impl Future<Item=(), Error=()>
	where T: RedisService + Send + Sync + 'static
{

	let addr = "127.0.0.1:12345".parse().unwrap();
	let listener = TcpListener::bind(&addr)
		.expect("unable to bind TCP listener");

	let server = listener.incoming()
		.map_err(|e| eprintln!("accept failed = {:?}", e))
		.for_each(move |sock| {
			tokio::spawn(handle_connection(sock, inst.clone()))
		});

	server
}

