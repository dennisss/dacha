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
use cnc_monitor::MonitorImpl;
use cnc_monitor_proto::cnc::MonitorIntoService;
use common::map;
use executor_multitask::RootResource;
use file::{project_path, LocalPathBuf};
use http::{
    static_file_handler::{StaticFileBody, StaticFileHandler},
    ServerHandler,
};
use parsing::ascii::AsciiString;
use rpc_util::NamedPortArg;
use web::WebServerHandler;

/// TODO: Move this to a shared crate.
pub struct ZipAllIterator<A, B> {
    a: A,
    b: B,
}

impl<T, Y, A: Iterator<Item = T>, B: Iterator<Item = Y>> Iterator for ZipAllIterator<A, B> {
    type Item = (Option<T>, Option<Y>);

    fn next(&mut self) -> Option<Self::Item> {
        let a = self.a.next();
        let b = self.b.next();
        if a.is_none() && b.is_none() {
            return None;
        }

        Some((a, b))
    }
}

pub fn bad_request() -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::BAD_REQUEST)
        .build()
        .unwrap()
}

pub fn not_found_request() -> http::Response {
    http::ResponseBuilder::new()
        .status(http::status_code::NOT_FOUND)
        .build()
        .unwrap()
}

fn extract_path_params(path: &str, pattern: &str) -> Option<HashMap<String, String>> {
    // TODO: Ensure that the path is first normalized

    let path_parts = path.split('/');
    let pattern_parts = pattern.split('/');

    let iter = ZipAllIterator {
        a: path_parts,
        b: pattern_parts,
    };

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

    tls_certificate: LocalPathBuf,
    tls_key: LocalPathBuf,

    #[arg(default = false)]
    make_fake_machines: bool,
}

struct HttpHandler {
    instance: Arc<MonitorImpl>,
    inner: WebServerHandler,
    data_handler: StaticFileHandler,
    rpc_handler: rpc::Http2RequestHandler,
}

impl HttpHandler {
    async fn handle_api_request(&self, request: http::Request) -> http::Response {
        match self.handle_api_request_impl(request).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("API Failure: {}", e);
                http::ResponseBuilder::new()
                    .status(http::status_code::INTERNAL_SERVER_ERROR)
                    .build()
                    .unwrap()
            }
        }
    }

    async fn handle_api_request_impl(&self, request: http::Request) -> Result<http::Response> {
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

        // /api/
        // /api/rpc/

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

        Ok(not_found_request())
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

impl HttpHandler {
    async fn handle_request_impl<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        if request.head.uri.path.as_str().starts_with("/api/") {
            return self.handle_api_request(request).await;
        }

        if let Some(path) = request.head.uri.path.as_str().strip_prefix("/rpc/") {
            request.head.uri.path = AsciiString::new(&format!("/{}", path));
            return self.rpc_handler.handle_request(request, context).await;
        }

        if let Some(path) = request.head.uri.path.as_str().strip_prefix("/data/") {
            request.head.uri.path = AsciiString::new(&format!("/{}", path));
            return self.data_handler.handle_request(request, context).await;
        }

        if request.head.uri.path.as_str().starts_with("/ui/") {
            request.head.uri.path = AsciiString::new("/");
        }

        self.inner.handle_request(request, context).await
    }
}

#[async_trait]
impl http::ServerHandler for HttpHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        self.handle_request_impl(request, context).await
    }

    // TODO: Passthrough connection handling to the rpc hnadler.
}

#[executor_main]
async fn main() -> Result<()> {
    /*
    {
        let start = Instant::now();

        let summary = cnc_monitor::program::ProgramSummary::create(&project_path!(
            "testdata/cnc/3DBenchy_0.2mm_PETG_MK3S_1h23m.gcode"
        ))
        .await?;

        let end = Instant::now();

        println!("{:?}", end - start);

        println!("{:#?}", summary.proto);

        let thumb = summary.best_thumbnail()?.unwrap();

        file::write(project_path!("thumb.jpg"), thumb).await?;

        return Ok(());
    }
    */

    /*
    {
        let gcode = file::read(project_path!(
            "testdata/cnc/3DBenchy_0.2mm_PETG_MK3S_1h23m.gcode"
        ))
        .await?;

        /*
        V1 benchmark:
        - Debug: 33s
        - Release: 2.9s

        V2 benchmark:
        - Debug: 12s
        - Releae: ~1.1s
        */

        println!("Loaded data!");

        let start = Instant::now();

        let mut parser = gcode::Parser::new();

        let mut iter = parser.iter(&gcode[..], true);

        // let mut input = gcode.as_bytes(); // &gcode[..];

        while let Some(e) = iter.next() {
            // println!("{:?}", e);

            if let gcode::Event::ParseError(kind) = e {
                eprintln!(
                    "Failed to parse! Line: {}: {:?}",
                    iter.parser().current_line_number(),
                    kind
                );
            }
        }

        let end = Instant::now();

        println!("{:?}", end - start);

        //

        return Ok(());
    }
    */

    let args = common::args::parse_args::<Args>()?;

    let service = RootResource::new();

    println!("Starting...");
    let start_time = Instant::now();

    let certificate_file = file::read(args.tls_certificate).await?.into();
    let private_key_file = file::read(args.tls_key).await?.into();

    let mut tls_options =
        crypto::tls::ServerOptions::recommended(certificate_file, private_key_file)?;

    let monitor =
        Arc::new(MonitorImpl::create(&args.local_data_dir, args.make_fake_machines).await?);
    service.register_dependency(monitor.clone()).await;

    let mut rpc_handler = rpc::Http2RequestHandler::new();
    rpc_handler.add_service(monitor.clone().into_service())?;

    service
        .register_dependency({
            let vars = json::Value::Object(map!(
                "rpc_port" => &json::Value::Number(args.port.value() as f64)
            ));

            let web_handler = web::WebServerHandler::new(web::WebServerOptions {
                pages: vec![web::WebPageOptions {
                    title: "CNC Monitor".into(),
                    path: "/".into(),
                    script_path: "built/pkg/cnc/monitor/app.js".into(),
                    vars: Some(vars),
                }],
            });

            let handler = HttpHandler {
                instance: monitor,
                inner: web_handler,
                data_handler: StaticFileHandler::new_with_options(
&args.local_data_dir,
                    StaticFileHandlerOptions {
                        // - The only untrusted files are user uploaded file blobs which we always
                        //   store with no extension.
                        // - .svg files must have a Content-Type else they won't be rendered in
                        //   browsers.
                        // - .zz files are used with Content-Encoding headers to decode.
                        trust_file_extension: true,
                    },
                ),
                rpc_handler,
            };

            let mut options = http::ServerOptions::default();
            options.port = Some(args.port.value());
            options.tls = Some(tls_options.clone());
            options.force_http2 = true;

            let web_server = http::Server::new(handler, options);
            Arc::new(web_server.start())
        })
        .await;

    // TODO: Actually wait for resource readiness and make this a standard metric
    // that we report.
    let end_time = Instant::now();

    println!("Ready! Startup took {:?}", end_time - start_time);

    service.wait().await
}
