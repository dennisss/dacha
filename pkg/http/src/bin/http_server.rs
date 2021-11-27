#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
extern crate parsing;

use common::async_std::fs;
use common::async_std::task;
use common::errors::*;
use http::header::*;
use http::status_code::*;

// TODO: Pipelining?

// If we send back using a chunked encoding,
async fn handle_request(req: http::Request) -> http::Response {
    println!("GOT: {:?}", req);

    let mut data = vec![];
    data.extend_from_slice(b"hello");
    // req.body.read_to_end(&mut data).await;

    // println!("READ: {:?}", data);

    http::ResponseBuilder::new()
        .status(OK)
        .header(CONTENT_TYPE, "text/plain")
        .body(http::BodyFromData(data))
        .build()
        .unwrap()
}

async fn run_server() -> Result<()> {
    let handler = http::static_file_handler::StaticFileHandler::new(common::project_dir());
    // let handler = http::HttpFn(handle_request);

    let certificate_file = fs::read(project_path!("testdata/certificates/server-ec.crt"))
        .await?
        .into();
    let private_key_file = fs::read(project_path!("testdata/certificates/server-ec.key"))
        .await?
        .into();

    let mut options = http::ServerOptions::default();
    options.tls = Some(crypto::tls::ServerOptions::recommended(
        certificate_file,
        private_key_file,
    )?);

    let server = http::Server::new(handler, options);
    server.run(8000).await
}

fn main() -> Result<()> {
    task::block_on(run_server())
}
