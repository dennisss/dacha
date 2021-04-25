#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
extern crate parsing;

use std::borrow::BorrowMut;
use std::convert::AsMut;
use std::convert::TryFrom;

use common::async_std::net::{TcpListener, TcpStream};
use common::async_std::prelude::*;
use common::async_std::task;
use common::errors::*;
use common::io::ReadWriteable;
use http::body::*;
use http::client::*;
use http::header::*;
use http::status_code::*;
use parsing::iso::*;
use http::response::*;
use http::request::*;

// TODO: Pipelining?

// If we send back using a chunked encoding,
async fn handle_request(req: Request) -> Response {
    println!("GOT: {:?}", req);

    let mut data = vec![];
    data.extend_from_slice(b"hello");
    // req.body.read_to_end(&mut data).await;

    // println!("READ: {:?}", data);

    // let res_headers = vec![
    //     Header::new("Content-Length".to_string(), format!("{}", data.len())),
    //     Header::new("Content-Type".to_string(), "text/plain".to_string()),
    // ];

    ResponseBuilder::new()
        .status(OK)
        .header(CONTENT_TYPE, "text/plain")
        // TODO: Move this to the Server internal implementation.
        .header(CONTENT_LENGTH, format!("{}", data.len()))
        .body(BodyFromData(data))
        .build()
        .unwrap()
}

async fn run_server() -> Result<()> {
    let handler = http::static_file_handler::StaticFileHandler::new("/home/dennis/workspace/dacha");
    // let handler = http::server::HttpFn(handle_request);
    let server = http::server::Server::new(8000, handler);
    server.run().await
}

fn main() -> Result<()> {
    task::block_on(run_server())
}
