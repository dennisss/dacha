
use mio::*;
use mio::net::{TcpListener, TcpStream};
use std::collections::HashMap;
use std::io::{Read, Write};
use slab::Slab;
use super::resp::*;


struct Connection {
	/// Monotonically increasing id (unlike the slab index, this will never repeat for two different connections)
	id: usize,

	socket: TcpStream,

	parser: RESPParser

}


pub fn run_server() {

	// Setup some tokens to allow us to identify which event is
	// for which socket.
	const SERVER: Token = Token(0);
	//const CLIENT: Token = Token(1);

	let addr = "127.0.0.1:13265".parse().unwrap();

	// Setup the server socket
	let server = TcpListener::bind(&addr).unwrap();

	// Create a poll instance
	let poll = Poll::new().unwrap();

	// Start listening for incoming connections
	poll.register(&server, SERVER, Ready::readable(),
				PollOpt::edge()).unwrap();

	/*
	// Setup the client socket
	let sock = TcpStream::connect(&addr).unwrap();

	// Register the socket
	poll.register(&sock, CLIENT, Ready::readable(),
				PollOpt::edge()).unwrap();
	*/

	let mut conns = Slab::<Connection>::new();

	// Create storage for events
	let mut events = Events::with_capacity(1024);

	//let mut conns = HashMap::new();
	let mut last_id = 0;

	let mut buf = [0u8; 512];

	/*
	let remove_conn = |num| {
		conns.remove(num);
		poll.remove(&Token(num));
		// Possibly also explicitly close the connection here
	};
	*/

	loop {
		//events.clear();
		poll.poll(&mut events, Some(std::time::Duration::from_millis(2000))).unwrap();

		//println!("EVENTS {}", events.len());
		for event in events.iter() {
			//println!("- EVENT {:?}", event);
			match event.token() {
				SERVER => {
					// Loop until out of connections to accept
					loop {
						// TODO: Loop until done all connection available
						// NOTE: Should no longer block
						let (mut sock, addr) = match server.accept() {
							Ok(v) => v,
							Err(e) => {
								match e.kind() {
									std::io::ErrorKind::WouldBlock => {
										// Normal
										println!("ACCEPT WOULD BLOCK");
									},
									_ => {
										println!("ACCEPT ERROR {:?}", e);	
									}
								};

								// Mainly in the clocking case we know we are done
								break;
							}
						};

						let id = last_id + 1; last_id = id;

						sock.set_nodelay(true).expect("Failed to set nodelay");

						println!("ACCEPTING {}", id);

						/*
						// Write initial response on connections
						let mut outbuf = Vec::new();
						let outobj = RESPObject::Nil;
						outobj.serialize_to(&mut outbuf);
						sock.write(&outbuf).expect("Write failed");
						*/
						let num = conns.insert(Connection {
							id,
							socket: sock,
							parser: RESPParser::new()
						});

						poll.register(
							&conns.get(num).unwrap().socket,
							Token(num + 1),
							Ready::readable() | Ready::hup(),
							PollOpt::edge()
						).expect("Failed to register poller");
					}
				}
				// Otherwise numbers starting at 1
				tok @ _ => {
					//println!("Client event {:?}", event.kind());

					// TODO: REad events should have an EOF flag on mac at least

					let num = tok.0 - 1;

					let mut conn = conns.get_mut(num).expect("NO SUCH CONN");

					// The alternative to this loop would be to set a level poller 
					loop {
						
						let num = match conn.socket.read(&mut buf) {
							Ok(v) => v,
							Err(e) => {
								match e.kind() {
									std::io::ErrorKind::WouldBlock => {
										// This should be normal
									},
									_ => {
										println!("GOT ERR {:?}", e);
										// Probably close the connection
									}
								}

								break;
							}
						};

						if num == 0 {
							// End of the file (TODO: Also clean up from the slab)
							// TODO: Also possible to do this earler if we see a Hup event (or also see if write returns 0?)
							poll.deregister(&conn.socket).expect("Failed to deregister sock");
							break;
						}

						//println!("READ {}", num);

						// TODO: Outbound will need to operate on a queue and waiting for the socket to become writeable

						if num > 4 {
							conn.socket.write(&b"+OK\r\n"[..]).expect("Write failed");
							continue;
						}

						let mut i = 0;
						let mut nread = 0;
						while i < num {
							let (n, obj) = match conn.parser.parse(&buf[i..num]) {
								Ok(v) => v,
								Err(e) => {
									eprintln!("{}", e);
									// TODO: In this case we can close the connection
									break;
								}
							};

							if n == 0 {
								println!("Made no progress");
							}

							i += n;

							if let Some(o) = obj {

								nread += 1;

								conn.socket.write(&b"+OK\r\n"[..]).expect("Write failed");

								//let cmd = o.to

								// Serve the request based on this one
							}

						}

						if nread > 1 {
							println!("READ {} at once", nread);
						}
					}
				}
			}
		}
		
	}

}