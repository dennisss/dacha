// TODO: Eventually this needs to be improved since there is nothing
// tieing a single browser instance to a set of camera data streams.

use std::time::Duration;
use std::sync::Arc;

use common::io::*;
use common::errors::*;
use websocket::*;
use http::status_code::OK;
use http::header::CONTENT_TYPE;
use http::static_file_handler::*;
use http_util::{PathRouter, not_found, bad_request};
use mocap_manager::side_channel::*;
use executor::channel::spsc;
use executor_multitask::impl_resource_passthrough;
use file::LocalPath;
use executor::sync::SyncMutex;

use crate::rpc_server::*;


pub struct AppHttpServer {
    server: http::server::ServerResource,
    port: u16,
}

impl_resource_passthrough!(AppHttpServer, server);

impl AppHttpServer {
    pub async fn create(data_dir: &LocalPath, service: Arc<dyn rpc::Service>, side_channel: Arc<DataSideChannel>) -> Result<Self> {
        let mut router: PathRouter<Box<dyn http::ServerHandler>> = PathRouter::default();

        router.add_route("/assets", true, Box::new(web::assets_handler()))?;

        // TODO: This directory also contains all the webview state data so probably not ideal to expose all that.
        let data_handler = StaticFileHandler::new_with_options(
            &data_dir,
            StaticFileHandlerOptions {
                trust_file_extension: true,
                mount_path: "/data".to_string(),
            },
        );
        router.add_route("/data", true, Box::new(data_handler))?;

        let web_handler = Arc::new(web::WebPageHandler::create(web::WebPageOptions {
            title: "Mocap Manager".into(),
            script_path: "built/pkg/vision/mocap/manager/app.js".into(),
            vars: Some(json::Value::Object(map! {
                "use_websocket_rpc" => &json::Value::Bool(true)
            }))
        }).await?);
        router.add_route("/", false, Box::new(web_handler.clone()))?;
        router.add_route("/ui", true, Box::new(web_handler.clone()))?;


        let handler = AppHttpHandler { router, service, side_channel };

        let mut options = http::ServerOptions::default();
        options.port = None;

        let server = http::Server::new(handler, options).bind().await?;
        
        let port = server.local_addr()?.port();

        Ok(Self {
            server: server.start(),
            port
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

struct AppHttpHandler {
    router: PathRouter<Box<dyn http::ServerHandler>>,
    service: Arc<dyn rpc::Service>,
    side_channel: Arc<DataSideChannel>
}

#[async_trait]
impl http::ServerHandler for AppHttpHandler {
    fn handle_connecting(&self, context: &mut http::ServerConnectionContext) -> bool {
        // Block non-local connections since only the UI should be accessing the server.
        // This is mainly a security measure.

        if !context.peer.ip().is_v4() {
            return false;
        }

        if !context.peer.ip().as_bytes().starts_with(&[ 127, 0, 0 ]) {
            return false;
        }

        true
    }

    async fn handle_request<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {

        // TODO: Check for authentication key.

        // TODO: CORS

        request.head.uri = match request.head.uri.normalized() {
            Ok(v) => v,
            Err(_) => return bad_request(),
        };

        if !request.head.uri.path.as_str().starts_with("/") {
            return bad_request();
        }

        let handler = match self.router.route(request.head.uri.path.as_str()) {
            Some((_, v)) => v,
            None => return not_found(),
        };

        handler.handle_request(request, context).await
    }

    async fn handle_upgrade(
        &self,
        req_head: http::RequestHead,
        reader: Box<dyn Readable>,
        mut writer: Box<dyn SharedWriteable>
    ) -> Result<()> {
        // TODO: Check for authentication key.

        let (on_message_sender, mut on_message_receiver) = spsc::bounded(10000);

        let socket = Arc::new(WebSocket::create_server(Arc::new(SocketHandler {
            on_message_sender: SyncMutex::new(on_message_sender)
        }), req_head, reader, writer).await?);

        let rpc_server = AppRpcServer::new(self.service.clone(), socket.clone());

        while let Ok(msg) = on_message_receiver.recv().await {
            if let Err(e) = rpc_server.handle_message(std::str::from_utf8(&msg)?) {
                println!("Failed to handle message: {}", e);
            }
        }

        Ok(())
    }
}

struct SocketHandler {
    on_message_sender: SyncMutex<spsc::Sender<Vec<u8>>>
}

#[async_trait]
impl WebSocketHandler for SocketHandler {
    async fn handle_message(&self, is_text: bool, data: &[u8]) {
        if is_text {
            let _ = self.on_message_sender.apply(|s| {
                s.try_send(data.to_vec());
            });
        }
    }
}
