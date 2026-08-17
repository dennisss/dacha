#[macro_use]
extern crate common;
extern crate http;
#[macro_use]
extern crate regexp_macros;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::fmt::Write;

use common::bytes::Bytes;
use common::errors::*;
use http::{
    static_file_handler::{StaticFileHandler, StaticFileHandlerOptions},
    ServerHandler,
};
use parsing::ascii::AsciiString;

/*
We will provide a standard way for programs to register web pages.

Provide a GRPC over websocket solution?

Specifying files:
- In the build system, the file must be explicitly linked via 'data' dependencies.
    - Generate a proto file that contains an allowlist from this.
*/

regexp!(HTML_TEMPLATE => "{{\\s*([a-zA-Z0-9_.-]+)\\s*}}");

// TODO: Use this everywhere.
const ASSETS_PATH_SUFFIX: &'static str = "/assets";

pub fn assets_handler() -> StaticFileHandler {
    let mut asset_options = StaticFileHandlerOptions::default();
    asset_options.trust_file_extension = true;
    asset_options.mount_path = ASSETS_PATH_SUFFIX.to_string();
    StaticFileHandler::new_with_options(file::project_dir(), asset_options)
}

/// Configuration for a single page to be
pub struct WebPageOptions {
    /// <title> to use for this page
    pub title: String,

    /// Relative path to the JavaScript file that should be executed on this
    /// page.
    ///
    /// TODO: Ideally have a way of linking this up.
    pub script_path: String,

    pub vars: Option<json::Value>,
}

/// Renders a single HTML page that never changes based on URL path.
pub struct WebPageHandler {
    page: Bytes
}

impl WebPageHandler {
    pub async fn create(options: WebPageOptions) -> Result<Self> {
        let vars = json::stringify(options.vars.as_ref().unwrap_or(&json::Value::Null))?;

        let contents = file::read_asset_to_str("pkg/web/index.html").await?.to_string();

        let mut new_page = String::new();

        let mut last_index = 0;
        let mut mat = HTML_TEMPLATE.exec(contents.as_str());
        while let Some(m) = mat {
            write!(&mut new_page, "{}", &contents[last_index..m.index()])?;
            last_index = m.last_index();

            let name = m.group_str(1).unwrap().unwrap();
            if name == "title" {
                write!(&mut new_page, "{}", options.title)?;
            } else if name == "bundle_path" {
                write!(&mut new_page, "{}/{}", ASSETS_PATH_SUFFIX, options.script_path)?;
            } else if name == "vars" {
                write!(&mut new_page, "{}", vars)?;
            } else {
                return Err(err_msg("Unknown template string"));
            }

            mat = m.next();
        }

        write!(&mut new_page, "{}", &contents[last_index..])?;

        Ok(Self { page: new_page.into() })
    }

    pub fn get(&self) -> Bytes {
        self.page.clone()
    }
}

#[async_trait]
impl http::ServerHandler for WebPageHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        http::ResponseBuilder::new()
            .status(http::status_code::OK)
            .header(http::header::CONTENT_TYPE, "text/html")
            .body(http::BodyFromData(self.page.clone()))
            .build()
            .unwrap()
    }
}
