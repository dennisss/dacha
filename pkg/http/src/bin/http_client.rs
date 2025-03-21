#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
extern crate parsing;
#[macro_use]
extern crate macros;

use std::convert::TryFrom;

use common::bytes::Bytes;
use common::errors::*;
use file::LocalPath;
use http::{ClientInterface, ClientRequestContext};
use http::{ClientResponseContext, SimpleClientOptions};
use parsing::iso::*;

// TODO: THe Google TLS server will timeout the connection if the SETTINGs
// packet isn't received fast enough.

/*
sudo sh -c 'echo -1 > /proc/sys/kernel/perf_event_paranoid'
perf record ./target/release/http_client
pprof -web target/release/http_client perf.data
*/

// TODO: Disallow Authorization like headers in in-secure channels

#[executor_main]
async fn main() -> Result<()> {
    // TODO: Follow redirects (301 and 302) or if Location is set

    /*
    {
        let mut options = http::ClientOptions::try_from("https://spanner.googleapis.com")?;
        let client = http::Client::create(options)?;

        let req = http::RequestBuilder::new()
            .method(http::Method::POST)
            .path("/google.spanner.admin.database.v1.DatabaseAdmin/CreateDatabase")
            .accept_trailers(true)
            // .header("grpc-timeout", "1S")
            .header("grpc-encoding", "identity")
            // .header("authorization", "Bearer value")
            // .header("Accept", "application/grpc+proto")
            .body(http::BodyFromData("hello".as_bytes()))
            .header("Content-Type", "application/grpc")
            // .header("Accept", "text/html")
            // .header("Accept-Encoding", "gzip")
            .build()?;

        let mut res = client.request(req, ClientRequestContext::default()).await?;
        println!("{:?}", res.head);

        let mut body = http::encoding::decode_content_encoding_body(&res.head.headers, res.body)?;

        let mut body_buf = vec![];
        body.read_to_end(&mut body_buf).await?;

        println!(
            "BODY\n{}",
            Latin1String::from_bytes(body_buf.into())
                .unwrap()
                .to_string()
        );

        let trailers = body.trailers().await?;
        println!("{:?}", trailers);

        return Ok(());
    }
    */

    /*
    {
        let mut options =
            http::ClientOptions::try_from("https://acme-staging-v02.api.letsencrypt.org")?;
        let client = http::Client::create(options).await?;

        // /directory

        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .path("/")
            .header("Accept", "text/html")
            .header("Accept-Encoding", "gzip")
            .build()?;

        let mut res = client
            .request(
                req,
                ClientRequestContext::default(),
                &mut ClientResponseContext::default(),
            )
            .await?;
        println!("{:?}", res.head);

        let mut body = http::encoding::decode_content_encoding_body(&res.head.headers, res.body)?;

        let mut body_buf = vec![];
        body.read_to_end(&mut body_buf).await?;

        println!(
            "BODY\n{}",
            Latin1String::from_bytes(body_buf.into())
                .unwrap()
                .to_string()
        );

        return Ok(());
    }
    */

    {
        let creds =
            crypto::tls::FileCredentialsLoader::create(&LocalPath::new("/tmp/tls_root/")).await?;

        let mut options = http::ClientOptions::try_from("http://10.1.0.43:10400")?;

        // ftkyy996zvsp8.node.testing.cluster.internal

        let mut tls_options = creds.client_options().get().as_ref().clone();

        // let mut tls_options = crypto::tls::ClientOptions::recommended();
        tls_options.certificate_request.trust_remote_certificate = true;
        options.backend_balancer.backend.tls = Some(tls_options.into());

        let client = http::Client::create(options).await?;

        let req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .path("/")
            .header("Accept", "text/html")
            .header("Accept-Encoding", "gzip")
            .build()?;

        let mut res = client
            .request(
                req,
                ClientRequestContext::default(),
                &mut ClientResponseContext::default(),
            )
            .await?;
        println!("{:?}", res.head);

        return Ok(());
    }

    /*
       // RSA 2048
       let certificate_file = file::read(project_path!("testdata/certificates/alice.crt"))
           .await?
           .into();
       let private_key_file = file::read(project_path!("testdata/certificates/alice.key"))
           .await?
           .into();

       tls_options.certificate_auth = Some(crypto::tls::CertificateAuthenticationOptions::create(
           certificate_file,
           private_key_file,
       )?);
    */

    let client = http::SimpleClient::new(SimpleClientOptions::default());

    let req = http::RequestBuilder::new()
        .method(http::Method::GET)
        .uri("https://127.0.0.1:10400/index.html")
        // .path("/index.html")
        .header("Accept", "text/html")
        .header("Accept-Encoding", "gzip")
        .build()?;

    let res = client
        .request(&req.head, Bytes::new(), &ClientRequestContext::default())
        .await?;
    println!("{:?}", res.head);

    // TODO: Read Content-Type to get the charset.

    println!(
        "BODY\n{}",
        Latin1String::from_bytes(res.body).unwrap().to_string()
    );

    return Ok(());
}
