#![feature(trait_upcasting)]

/*
HTTP Paths:
- '/', '/ui/.*' : Redirect to the HTML page
- '/api' : Internally processed
- '/assets' : Static non-private data linked with the
    - TODO: Ideally disallow most things to be downloaded aside from legitate assets
- '/data/'
    - TODO: Limit me to just the files and camera data
    - Eventually will require strict authentication

*/

#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::{collections::HashMap, sync::Arc, time::Instant};

use base_error::*;
use base_util::zip_all::ZipAllIterator;
use cnc_monitor::MonitorImpl;
use cnc_monitor_proto::cnc::MonitorIntoService;
use executor_multitask::RootResource;
use file::LocalPathBuf;
use http::static_file_handler::StaticFileHandlerOptions;
use http::{
    static_file_handler::{StaticFileBody, StaticFileHandler},
    ServerHandler,
};
use http_util::{bad_request, not_found, internal_server_error};
use parsing::ascii::AsciiString;
use rpc_util::NamedPortArg;
use cluster_client::ClusterMetaClient;

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: [
        # Static file serving.
        {
            path: "/"
            is_directory: false
            principals: ["authenticated"]
        },
        {
            path: "/ui"
            is_directory: true
            principals: ["authenticated"]
        },

        # 
        {
            path: "/data"
            is_directory: true
            principals: []
        },

        {
            path: "/api"
            is_directory: true
            principals: []
        },

        {
            path: "/rpc/cnc.Monitor"
            is_directory: true
            principals: ["group:cluster-admins"]
        }
    ]
"#;

fn extract_path_params(path: &str, pattern: &str) -> Option<HashMap<String, String>> {
    // TODO: Ensure that the path is first normalized

    let path_parts = path.split('/');
    let pattern_parts = pattern.split('/');

    let iter = ZipAllIterator::new(path_parts, pattern_parts);

    let mut params = HashMap::default();

    for (path_part, pattern_part) in iter {
        let path_part = match path_part {
            Some(v) => v,
            None => return None,
        };

        let pattern_part = match pattern_part {
            Some(v) => v,
            None => return None,
        };

        if let Some(param_name) = pattern_part.strip_prefix(':') {
            params.insert(param_name.to_string(), path_part.to_string());
        } else if path_part != pattern_part {
            return None;
        }
    }

    Some(params)
}

#[derive(Args)]
struct Args {
    port: NamedPortArg,

    local_data_dir: LocalPathBuf,

    #[arg(default = false)]
    make_fake_machines: bool,
}

struct ApiHttpHandler {
    instance: Arc<MonitorImpl>,
}

impl ApiHttpHandler {
    async fn handle_request_impl<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>
    ) -> Result<http::Response> {
        /*
        - Check for 'auth_key' cookie
        - Base64 decode
        - Lookup session
        - Must de


        User authentication will be required
        - An initial user will be created on first launch
        - Initial password printed to console


        */

        // TODO: Finish implementing all

        /*
        Securing things:
        1. Switch to a router implementation so that we can ensure each path has access control.
        2. Lock down which local files can be accessed via the static file handler.
        */

        let path = request.head.uri.path.as_str();

        if path == "/api/files/upload" {
            if request.head.method != http::Method::POST {
                return http::ResponseBuilder::new()
                    .status(http::status_code::METHOD_NOT_ALLOWED)
                    .build();
            }

            let mut query = match Self::parse_query(&request) {
                Ok(v) => v,
                Err(e) => return Ok(bad_request()),
            };

            let id = match query.remove("id").and_then(|v| v.parse::<u64>().ok()) {
                Some(v) => v,
                None => return Ok(bad_request()),
            };

            let size = match request.body.len() {
                Some(v) => v,
                None => return Ok(bad_request()),
            };

            self.instance
                .files()
                .upload_file(id, size as u64, request.body)
                .await?;

            return http::ResponseBuilder::new()
                .status(http::status_code::OK)
                .build();
        }

        /*
        if let Some(mut params) = extract_path_params(path, "/api/files/:file_id/thumbnail") {
            let file_id = match params.remove("file_id").unwrap().parse::<u64>() {
                Ok(v) => v,
                Err(e) => return Ok(bad_request()),
            };

            // TODO: Must convert rpc errors to http errors.
            // TODO: Hold this lock while the body is running.
            let file_lock = self.instance.files().lookup(file_id)?;

            // TODO: Handle errors from this.
            let body = StaticFileBody::open(&file_lock.thumbnail_path()).await?;

            // TODO: Need a Content-Type. Also need to disable all caching.
            return http::ResponseBuilder::new()
                .status(http::status_code::OK)
                .body(Box::new(body))
                .build();
        }
        */

        /*
        /api/files/:file_id/raw
        /api/files/:file_id/thumbnail
        */

        if let Some(mut params) =
            extract_path_params(path, "/api/machines/:machine_id/cameras/:camera_id/stream")
        {
            let machine_id = match params.remove("machine_id").unwrap().parse::<u64>() {
                Ok(v) => v,
                Err(e) => return Ok(bad_request()),
            };

            let camera_id = match params.remove("camera_id").unwrap().parse::<u64>() {
                Ok(v) => v,
                Err(e) => return Ok(bad_request()),
            };

            return self.instance.get_camera_feed(machine_id, camera_id).await;
        }

        Ok(not_found())
    }

    fn parse_query(request: &http::Request) -> Result<HashMap<String, String>> {
        let mut out = HashMap::new();
        let data = match &request.head.uri.query {
            Some(v) => v.as_str(),
            None => return Ok(out),
        };

        let mut parser = http::query::QueryParamsParser::new(data.as_bytes());

        for (key, value) in parser.next() {
            let key = key.to_utf8_str()?.to_string();
            let value = value.to_utf8_str()?.to_string();
            if out.contains_key(&key) {
                return Err(err_msg("Duplicate key in query"));
            }

            out.insert(key, value);
        }

        Ok(out)
    }
}

#[async_trait]
impl http::ServerHandler for ApiHttpHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        match self.handle_request_impl(request, context).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("API Failure: {}", e);
                internal_server_error()
            }
        }
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    println!("Starting...");
    let start_time = Instant::now();

    let client = ClusterMetaClient::create_from_environment().await?;

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = cluster_client::ClusterServer::new(args.port.value(), acl, client)?;

    let monitor =
        Arc::new(MonitorImpl::create(&args.local_data_dir, args.make_fake_machines).await?);
    service.register_dependency(monitor.clone()).await;
    server.add_service(monitor.clone().into_service())?;

    let data_handler = StaticFileHandler::new_with_options(
        &args.local_data_dir,
        StaticFileHandlerOptions {
            // - The only untrusted files are user uploaded file blobs which we always
            //   store with no extension.
            // - .svg files must have a Content-Type else they won't be rendered in
            //   browsers.
            // - .zz files are used with Content-Encoding headers to decode.
            trust_file_extension: true,

            mount_path: "/data".to_string(),
        },
    );
    server.add_request_handler("/data", true, data_handler)?;


    let web_handler = Arc::new(web::WebPageHandler::create(web::WebPageOptions {
        title: "CNC Monitor".into(),
        script_path: "built/pkg/cnc/monitor/app.js".into(),
        vars: None,
    }).await?);
    server.add_request_handler("/", false, web_handler.clone())?;
    server.add_request_handler("/ui", true, web_handler.clone())?;

    server.add_request_handler("/api", true, ApiHttpHandler {
        instance: monitor,
    });

    // TODO: Actually wait for resource readiness and make this a standard metric
    // that we report.
    let end_time = Instant::now();

    println!("Ready! Startup took {:?}", end_time - start_time);

    service.wait().await
}
