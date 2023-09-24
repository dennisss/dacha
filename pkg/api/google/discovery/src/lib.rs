#[macro_use]
extern crate macros;

mod compiler;
mod format;

use common::line_builder::LineBuilder;
use compiler::*;
pub use format::*;

use std::collections::HashMap;
use std::time::Instant;

use common::bytes::Bytes;
use common::errors::*;
use file::project_path;
use file::LocalPathBuf;
use reflection::ParseFrom;

const DISCOVERY_ROOT_URL: &'static str = "https://discovery.googleapis.com/discovery/v1/apis";

/*
TODO: Eventualyl generalize HTTP caching to disk:
- Something similar to https://www.chromium.org/developers/design-documents/network-stack/http-cache/
- Also need to handle byte ranges.
- Can also be used to cache RPC conversations.
- Some type of cache invalidation mechanism for other applications.
- Support for multiple versions.
- Caching of requests so that we know how to reproduce a query (excluding secret keys)

*/

pub struct ClientOptions {
    /// If present, we will use this directory to cache requests to read and
    /// write HTTP responses.
    pub cache_directory: Option<LocalPathBuf>,

    /// If true, only allow API responses to be served from the local cache (no
    /// external requests are made).
    pub cached_only: bool,
}

pub struct Client {
    options: ClientOptions,
    http_client: http::SimpleClient,
}

impl Client {
    pub fn cache_only() -> Result<Self> {
        Self::new(ClientOptions {
            cache_directory: Some(project_path!("third_party/google/discovery/data")),
            cached_only: true,
        })
    }

    pub fn new(options: ClientOptions) -> Result<Self> {
        let http_client = http::SimpleClient::new(http::SimpleClientOptions::default());
        Ok(Self {
            options,
            http_client,
        })
    }

    pub async fn compile_all(&mut self) -> Result<String> {
        let root = self.get::<DirectoryList>(DISCOVERY_ROOT_URL).await?;

        let mut lines = LineBuilder::new();
        for item in root.items {
            let allowlist = item.id == "dns:v1" || item.id == "storage:v1";
            if !allowlist {
                continue;
            }

            let desc = self.get::<RestDescription>(&item.discoveryRestUrl).await?;

            let module_name = item.id.replace(":", "_");

            lines.add(format!("pub mod {} {{", module_name));
            lines.add("use super::*;");
            lines.add(Compiler::compile(&desc)?);
            lines.add("}");
            lines.nl();
        }

        Ok(lines.to_string())
    }

    async fn get<T: for<'a> ParseFrom<'a>>(&mut self, url: &str) -> Result<T> {
        let raw = self.get_raw(url).await?;
        let data = std::str::from_utf8(&raw)?;
        let value = json::parse(data)?;

        let out = T::parse_from(json::ValueParser::new(&value))?;

        Ok(out)
    }

    async fn get_raw(&mut self, url: &str) -> Result<Bytes> {
        if self.options.cached_only {
            let cache_dir = self
                .options
                .cache_directory
                .as_ref()
                .ok_or_else(|| err_msg("cache_only must specifiy a cache_dir"))?;

            let path = cache_dir.join(format!("{}.json", base_radix::hex_encode(url.as_bytes())));
            return Ok(file::read(path).await?.into());
        }

        // TODO: Add 'if-not-modified' like headers.
        let req = http::RequestBuilder::new()
            .uri(url)
            .method(http::Method::GET)
            .build()?;

        let res = self
            .http_client
            .request(
                &req.head,
                Bytes::new(),
                &http::ClientRequestContext::default(),
            )
            .await?;
        if res.head.status_code != http::status_code::OK {
            return Err(err_msg("Request failed"));
        }

        // TODO: Check content type.

        // Write to cache
        if let Some(cache_dir) = &self.options.cache_directory {
            // TODO: Normalize the URL before getting from the cache.
            let path = cache_dir.join(format!("{}.json", base_radix::hex_encode(url.as_bytes())));
            file::write(path, &res.body).await?;
        }

        Ok(res.body)
    }
}
