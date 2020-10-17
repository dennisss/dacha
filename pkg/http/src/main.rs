#![feature(core_intrinsics, async_await, trait_alias)]

#[macro_use] extern crate common;
extern crate libc;
extern crate http;
extern crate parsing;

use common::errors::*;

use std::io;
use std::io::{Read, Write, Cursor};
use std::sync::Arc;
use std::thread;
use std::convert::TryFrom;
use std::str::FromStr;
use std::borrow::BorrowMut;
use std::convert::AsMut;

use common::bytes::Bytes;
use http::client::*;

use common::async_std::net::{TcpListener, TcpStream};
use common::async_std::task;
use common::async_std::prelude::*;
use parsing::iso::*;

use common::errors::*;
//use http::reader::*;
//use http::uri::*;
//use http::uri_parser::*;
//use http::dns::*;
use http::spec::*;
use http::message::*;
use http::body::*;
use http::chunked::*;
use http::transfer_encoding::*;
use http::status_code::*;
use http::header::*;

// TODO: Pipelining?


// If we send back using a chunked encoding, 
async fn handle_request(mut req: Request) -> Response {

	println!("GOT: {:?}", req);

	let mut data = vec![];
	data.extend_from_slice(b"hello");
	// req.body.read_to_end(&mut data).await;

	// println!("READ: {:?}", data);

	let res_headers = vec![
		HttpHeader::new("Content-Length".to_string(),
						format!("{}", data.len())),
		HttpHeader::new("Content-Type".to_string(), "text/plain".to_string())
	];

	ResponseBuilder::new()
		.status(OK)
		.header(CONTENT_TYPE, "text/plain")
		.header(CONTENT_LENGTH, format!("{}", data.len()))
		.body(BodyFromData(data))
		.build()
		.unwrap()
}

// Implementing stuff for Body.


use compression::gzip::*;
use common::io::ReadWriteable;

async fn run_client() -> Result<()> {
	// TODO: Follow redirects (301 and 302) or if Location is set

	let mut client = Client::create("http://www.google.com")?;

	let req = RequestBuilder::new()
		.method(Method::GET)
		.uri("/index.html")
		.header("Accept", "text/html")
		.header("Host", "www.google.com")
		.header("Accept-Encoding", "gzip")
		.body(EmptyBody())
		.build()?;

	let mut res = client.request(req).await?;
	println!("{:?}", res.head);

	let content_encoding = http::header_parser::parse_content_encoding(
		&res.head.headers)?;
	if content_encoding.len() > 1 {
		return Err(err_msg("More than one Content-Encoding not supported"));
	}


	let mut body_buf = vec![];
	res.body.read_to_end(&mut body_buf).await?;

	if content_encoding.len() == 1 {
		if content_encoding[0] == "gzip" {
			let mut c = std::io::Cursor::new(&body_buf);
			let gz = read_gzip(&mut c)?;

			body_buf = gz.data;
		}
		else {
			return Err(format_err!("Unsupported content-encoding: {}",
								   content_encoding[0]));
		}
	}

	// TODO: Read Content-Type to get the charset.

	println!("BODY\n{}",
			 Latin1String::from_bytes(body_buf.into()).unwrap().to_string());

	return Ok(());
}

async fn run_server() -> Result<()> {
	let server = http::server::HttpServer::new(
		8000, http::server::HttpFn(handle_request));
	server.run().await
}

fn main() -> Result<()> {
	task::block_on(run_server())
}

