#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
extern crate parsing;

use std::borrow::BorrowMut;
use std::convert::AsMut;
use std::convert::TryFrom;
use std::io;
use std::io::{Cursor, Read, Write};
use std::str::FromStr;
use std::sync::Arc;
use std::thread;

use common::async_std::net::{TcpListener, TcpStream};
use common::async_std::prelude::*;
use common::async_std::task;
use common::bytes::Bytes;
use common::errors::*;
use common::errors::*;
use common::io::ReadWriteable;
use compression::gzip::*;
use http::body::*;
use http::chunked::*;
use http::client::*;
use http::header::*;
use http::message::*;
use http::spec::*;
use http::status_code::*;
use http::transfer_encoding::*;
use parsing::iso::*;
use http::response::*;
use http::request::*;

// TODO: Pipelining?

// If we send back using a chunked encoding,
async fn handle_request(mut req: Request) -> Response {
    println!("GOT: {:?}", req);

    let mut data = vec![];
    data.extend_from_slice(b"hello");
    // req.body.read_to_end(&mut data).await;

    // println!("READ: {:?}", data);

    let res_headers = vec![
        HttpHeader::new("Content-Length".to_string(), format!("{}", data.len())),
        HttpHeader::new("Content-Type".to_string(), "text/plain".to_string()),
    ];

    ResponseBuilder::new()
        .status(OK)
        .header(CONTENT_TYPE, "text/plain")
        .header(CONTENT_LENGTH, format!("{}", data.len()))
        .body(BodyFromData(data))
        .build()
        .unwrap()
}

async fn run_server() -> Result<()> {
    let server = http::server::HttpServer::new(8000, http::server::HttpFn(handle_request));
    server.run().await
}

fn main() -> Result<()> {
    task::block_on(run_server())
}
