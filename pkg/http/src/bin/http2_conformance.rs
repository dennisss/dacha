/*

git clone git@github.com:summerwind/h2spec.git
go run cmd/h2spec/h2spec.go --port 8888 --struct hpack/4.2
*/

extern crate http;
extern crate common;

use common::errors::*;
use common::async_std::task;
use http::header::*;
use http::status_code::*;
use http::server::Server;

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



async fn run_server() -> Result<()> {
    let handler = http::server::HttpFn(handle_request);
    let server = Server::new(8888, handler);
    server.run().await
}

fn main() -> Result<()> {
    task::block_on(run_server())
}