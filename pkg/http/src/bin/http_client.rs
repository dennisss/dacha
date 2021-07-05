#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
extern crate parsing;

use common::async_std::task;
use common::errors::*;
use parsing::iso::*;

// TODO: THe Google TLS server will timeout the connection if the SETTINGs packet isn't received fast enough.

async fn run_client() -> Result<()> {
    // TODO: Follow redirects (301 and 302) or if Location is set

    let client = http::Client::create(
        http::ClientOptions::from_authority("google.com")?.set_secure(true))?;

    let req = http::RequestBuilder::new()
        .method(http::Method::GET)
        .path("/index.html")
        .header("Accept", "text/html")
        .header("Accept-Encoding", "gzip")
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
