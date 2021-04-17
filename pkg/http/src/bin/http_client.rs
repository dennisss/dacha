#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
extern crate parsing;

use std::convert::TryFrom;
use std::str::FromStr;
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
use http::encoding::*;
use http::request::*;
use http::method::*;
use parsing::iso::*;


async fn run_client() -> Result<()> {
    // TODO: Follow redirects (301 and 302) or if Location is set

    let client = Client::create("http://www.google.com")?;

    let req = RequestBuilder::new()
        .method(Method::GET)
        .uri("/index.html")
        .header("Accept", "text/html")
        .header("Host", "www.google.com")
        .header("Accept-Encoding", "gzip")
        .body(EmptyBody()) // TODO: Should we make this the default?
        .build()?;

    let mut res = client.request(req).await?;
    println!("{:?}", res.head);

    let mut body = http::encoding::decode_content_encoding_body(&res.head.headers, res.body)?;

    let mut body_buf = vec![];
    body.read_to_end(&mut body_buf).await?;

    // TODO: Read Content-Type to get the charset.

    println!(
        "BODY\n{}",
        Latin1String::from_bytes(body_buf.into())
            .unwrap()
            .to_string()
    );

    return Ok(());
}

fn main() -> Result<()> {
    task::block_on(run_client())
}
