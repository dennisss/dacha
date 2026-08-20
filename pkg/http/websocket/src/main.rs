#![feature(core_intrinsics, trait_alias)]

/*
cargo run --bin websocket -- --port=8123
*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate file;
#[macro_use]
extern crate macros;

use std::time::Duration;
use std::sync::Arc;

use common::io::*;
use common::errors::*;
use http::header::*;
use http::status_code::*;

use websocket::*;


const TESTPAGE: &'static str = r#"
<!doctype html>
<html>
    <head>
        <script>
            const socket = new WebSocket('ws://localhost:8123/ws');

            socket.addEventListener('open', (event) => {
                console.log('Connected to server');
                socket.send('Hello Server!'); // Send a message
            });

            socket.addEventListener('message', (event) => {
                console.log('Message from server: ', event.data);
            });

            socket.addEventListener('error', (error) => {
                console.error('WebSocket Error: ', error);
            });

            socket.addEventListener('close', (event) => {
                console.log('Connection closed', event.reason);
            });

        </script>
    </head>

    <body>
    </body>
</html>
"#;



struct Service {}

#[async_trait]
impl http::ServerHandler for Service {
    async fn handle_request<'a>(
        &self,
        req: http::Request,
        ctx: http::ServerRequestContext<'a>,
    ) -> http::Response {

        http::ResponseBuilder::new()
            .status(OK)
            .header(CONTENT_TYPE, "text/html")
            .body(http::BodyFromData(TESTPAGE))
            .build()
            .unwrap()
    }

    async fn handle_upgrade(
        &self,
        req_head: http::RequestHead,
        reader: Box<dyn Readable>,
        mut writer: Box<dyn SharedWriteable>
    ) -> Result<()> {
        println!("GOT UPGRADE: {:?}", req_head);

        let mut stream = WebSocket::create_server(
            Arc::new(SocketHandler {}),
            req_head, reader, writer).await?;
        println!("Accepted!");

        loop {
            println!("Write!");
            stream.write_binary(b"Hello!").await?;
            executor::sleep(Duration::from_secs(1)).await?;
        }

        Ok(())
    }
}

struct SocketHandler {

}

#[async_trait]
impl WebSocketHandler for SocketHandler {
    async fn handle_message(&self, is_text: bool, data: &[u8]) {

    }
}


#[derive(Args)]
struct Args {
    port: u16,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let handler = Service {};

    let mut options = http::ServerOptions::default();
    options.port = Some(args.port);

    let server = http::Server::new(handler, options);

    executor_multitask::wait_for_main_resource(server.start()).await
}
