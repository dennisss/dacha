/*

git clone git@github.com:summerwind/h2spec.git
go run cmd/h2spec/h2spec.go --port 8888 --struct hpack/4.2
*/

extern crate common;
extern crate http;
#[macro_use]
extern crate macros;

use common::errors::*;
use http::header::*;
use http::server::Server;
use http::status_code::*;

async fn handle_request(mut req: http::Request) -> http::Response {
    println!("REQUEST HANDLER GOT: {:?}", req);

    let mut data = vec![];
    if let Err(e) = req.body.read_to_end(&mut data).await {
        println!("FAILED TO READ BODY: {:?}", e);
        return http::ResponseBuilder::new()
            .status(INTERNAL_SERVER_ERROR)
            .build()
            .unwrap();
    }

    if let Err(e) = req.body.trailers().await {
        println!("FAILED TO READ TRAILERS: {:?}", e);
        return http::ResponseBuilder::new()
            .status(INTERNAL_SERVER_ERROR)
            .build()
            .unwrap();
    }

    let mut data = vec![];
    data.extend_from_slice(b"Hello World!");
    // req.body.read_to_end(&mut data).await;

    http::ResponseBuilder::new()
        .status(OK)
        .header(CONTENT_TYPE, "text/plain")
        .body(http::BodyFromData(data))
        .build()
        .unwrap()
}

#[executor_main]
async fn main() -> Result<()> {
    let handler = http::HttpFn(handle_request);
    let server = Server::new(handler, http::ServerOptions::default());
    server.run(8888).await
}
