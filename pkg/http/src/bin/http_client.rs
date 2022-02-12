#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
extern crate parsing;

use std::convert::TryFrom;

use common::async_std::fs;
use common::async_std::task;
use common::errors::*;
use http::{ClientInterface, ClientRequestContext};
use parsing::iso::*;

// TODO: THe Google TLS server will timeout the connection if the SETTINGs
// packet isn't received fast enough.

async fn run_client() -> Result<()> {
    // TODO: Follow redirects (301 and 302) or if Location is set

    let certificate_file = fs::read(project_path!("testdata/certificates/alice.crt"))
        .await?
        .into();
    let private_key_file = fs::read(project_path!("testdata/certificates/alice.key"))
        .await?
        .into();

    let mut options = http::ClientOptions::try_from("https://localhost:8000")?;
    let tls_options = options.backend_balancer.backend.tls.as_mut().unwrap();

    tls_options.certificate_request.trust_remote_certificate = true;

    tls_options.certificate_auth = Some(crypto::tls::CertificateAuthenticationOptions::create(
        certificate_file,
        private_key_file,
    )?);

    let client = http::Client::create(options)?;

    let req = http::RequestBuilder::new()
        .method(http::Method::GET)
        .path("/index.html")
        .header("Accept", "text/html")
        .header("Accept-Encoding", "gzip")
        .build()?;

    let mut res = client.request(req, ClientRequestContext::default()).await?;
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
