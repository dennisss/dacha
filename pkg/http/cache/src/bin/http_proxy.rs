// Simple service which acts as an HTTP 1.1 Proxy.
// It only supports GET requests and will
// -
/*
cargo run --bin http_proxy --release -- --port=9000 --cache_dir=/opt/dacha/http_proxy
*/

#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate http;
#[macro_use]
extern crate file;
#[macro_use]
extern crate macros;

use std::sync::Arc;

use common::errors::*;
use common::io::Writeable;
use executor_multitask::*;
use file::LocalFile;
use file::LocalFileOpenOptions;
use file::LocalPathBuf;
use http::header::*;
use http::status_code::*;
use http_cache::DiskCache;
use http_util::{bad_request, internal_server_error};
use parsing::ascii::AsciiString;

#[derive(Args)]
struct Args {
    port: u16,

    cache_dir: LocalPathBuf,

    /// If true, only read from the cache and return NOT_FOUND for everything
    /// else.
    #[arg(default = false)]
    cache_only: bool,
}

struct Service {
    cache: DiskCache,
}

impl Service {
    async fn handle_request_impl<'a>(
        &self,
        mut req: http::Request,
        ctx: http::ServerRequestContext<'a>,
    ) -> Result<http::Response> {
        // TODO: Prevent cyclic requests back to ourselves.
        // Add something like a 'X-Proxy-' header.

        // TODO: Just normalize the 'scheme' based on whether or not TLS was used.
        // (normalize this lower down in the http library)

        // NOTE: Currently a proxy request and a regular request can only be
        // differentiated by whether the 'scheme' is set it in the uri (which should
        // only be the case for absolute-uri form requests).
        if req.head.method != http::Method::GET
            || req.head.uri.scheme.is_none()
            || req.head.uri.authority.is_none()
        {
            eprintln!(
                "Bad request received: {:?} {}",
                req.head.method,
                req.head.uri.to_string()?
            );

            return Ok(bad_request());
        }

        if let http::uri::Host::IP(_) = req.head.uri.authority.as_ref().unwrap().host {
            eprintln!(
                "Requesting IP: {:?} {}",
                req.head.method,
                req.head.uri.to_string()?
            );
            return Ok(bad_request());
        }

        // TODO: Sometimes we get failures so ideally auto-retry.
        // "E: Failed to fetch http://archive.raspberrypi.com/debian/pool/main/libp/libpisp/libpisp1_1.1.0-1_arm64.deb  500  Internal Server Error [IP: 172.17.0.1 9000]"
        let request = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri2(req.head.uri.clone())
            .body(http::EmptyBody())
            .build()?;

        self.cache.request(request).await
    }
}

#[async_trait]
impl http::ServerHandler for Service {
    async fn handle_request<'a>(
        &self,
        req: http::Request,
        ctx: http::ServerRequestContext<'a>,
    ) -> http::Response {
        let res = match self.handle_request_impl(req, ctx).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Request failed: {}", e);
                internal_server_error()
            }
        };

        res
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let client = http::SimpleClient::new(http::SimpleClientOptions::default());

    let cache = DiskCache::open(client, &args.cache_dir).await?;

    let service = Service { cache };

    let mut options = http::ServerOptions::default();
    options.port = Some(args.port);
    let server = http::Server::new(service, options);

    let mut root = RootResource::new();
    root.register_dependency(Arc::new(server.start())).await;

    root.wait().await
}
