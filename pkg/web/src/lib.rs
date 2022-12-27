#[macro_use]
extern crate common;
extern crate http;
#[macro_use]
extern crate regexp_macros;
extern crate automata;
extern crate parsing;
#[macro_use]
extern crate macros;
extern crate json;
#[macro_use]
extern crate file;

use std::fmt::Write;

use common::errors::*;
use http::{static_file_handler::StaticFileHandler, ServerHandler};
use parsing::ascii::AsciiString;

/*
We will provide a standard way for programs to register web pages.

There are a few use-cases to support:
- General web serving (where RPC servicing is more secondary)
- Servers which are primarily for web
- Could

Constraints:
- gRPC should always be mounted as the root thing.
- Supporting an API in a web app should generally require running in on a separate port or domain
-

Provide a GRPC over websocket solution?

Specifying files:
- In the build system, the file must be explicitly linked via 'data' dependencies.
    - Generate a proto file that contains an allowlist from this.
*/

regexp!(HTML_TEMPLATE => "{{\\s*([a-zA-Z0-9_.-]+)\\s*}}");

const ASSETS_PATH_SUFFIX: &'static str = "/assets";

pub struct WebServerOptions {
    pub pages: Vec<WebPageOptions>,
}

/// Configuration for a single page to be
pub struct WebPageOptions {
    /// <title> to use for this page
    pub title: String,

    /// Url path at which this page is accessible. Should be absolute and
    /// starting with '/'.
    pub path: String,

    /// Relative path to the JavaScript file that should be executed on this
    /// page.
    pub script_path: String,

    pub vars: Option<json::Value>,
}

pub struct WebServerHandler {
    options: WebServerOptions,
    assets_handler: StaticFileHandler,
}

impl WebServerHandler {
    pub fn new(options: WebServerOptions) -> Self {
        Self {
            options,
            assets_handler: StaticFileHandler::new(file::project_dir()),
        }
    }

    async fn handle_request_impl<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        match self.handle_request_with_result(request, context).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error while handling request: {}", e);
                http::ResponseBuilder::new()
                    .status(http::status_code::INTERNAL_SERVER_ERROR)
                    .build()
                    .unwrap()
            }
        }
    }

    async fn handle_request_with_result<'a>(
        &self,
        mut request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> Result<http::Response> {
        let path = request.head.uri.path.as_str();

        if let Some(path) = path.strip_prefix(ASSETS_PATH_SUFFIX) {
            if path.starts_with("/") {
                request.head.uri.path = AsciiString::from(path).unwrap();
                return Ok(self.assets_handler.handle_request(request, context).await);
            }
        }

        let page = {
            let mut found_page = None;
            for page in &self.options.pages {
                if page.path == path {
                    found_page = Some(page);
                    break;
                }
            }

            if let Some(page) = found_page {
                page
            } else {
                return http::ResponseBuilder::new()
                    .status(http::status_code::NOT_FOUND)
                    .build();
            }
        };

        let vars = json::stringify(page.vars.as_ref().unwrap_or(&json::Value::Null))?;

        let contents = file::read_to_string(project_path!("pkg/web/index.html")).await?;

        let mut new_page = String::new();

        let mut last_index = 0;
        let mut mat = HTML_TEMPLATE.exec(contents.as_str());
        while let Some(m) = mat {
            write!(&mut new_page, "{}", &contents[last_index..m.index()])?;
            last_index = m.last_index();

            let name = m.group_str(1).unwrap().unwrap();
            if name == "title" {
                write!(&mut new_page, "{}", page.title)?;
            } else if name == "bundle_path" {
                write!(&mut new_page, "{}/{}", ASSETS_PATH_SUFFIX, page.script_path)?;
            } else if name == "vars" {
                write!(&mut new_page, "{}", vars)?;
            } else {
                return Err(err_msg("Unknown template string"));
            }

            mat = m.next();
        }

        write!(&mut new_page, "{}", &contents[last_index..])?;

        return http::ResponseBuilder::new()
            .status(http::status_code::OK)
            .header(http::header::CONTENT_TYPE, "text/html")
            .body(http::BodyFromData(new_page))
            .build();
    }
}

#[async_trait]
impl http::ServerHandler for WebServerHandler {
    async fn handle_request<'a>(
        &self,
        request: http::Request,
        context: http::ServerRequestContext<'a>,
    ) -> http::Response {
        self.handle_request_impl(request, context).await
    }
}
