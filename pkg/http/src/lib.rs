#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate parsing;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate regexp_macros;
extern crate crypto;
extern crate net;

mod backoff;
mod body;
mod chunked;
mod chunked_syntax;
mod client;
mod common_syntax;
mod dns;
pub mod encoding;
mod encoding_syntax;
pub mod header;
mod header_syntax;
mod headers;
mod hpack;
pub mod message;
mod message_body;
mod message_syntax;
mod method;
mod proto;
pub mod query;
mod reader;
mod request;
mod response;
pub mod server;
mod spec;
pub mod static_file_handler;
pub mod status_code;
pub mod uri;
pub mod uri_syntax;
mod v1;
mod v2;

// Public exports.
pub use crate::body::{Body, BodyFromData, BodyFromParts, EmptyBody, WithTrailers};
pub use crate::client::{
    Client, ClientInterface, ClientOptions, ResolvedEndpoint, Resolver, ResolverChangeListener,
};
pub use crate::header::{Header, Headers};
pub use crate::method::Method;
pub use crate::request::{Request, RequestBuilder, RequestHead};
pub use crate::response::{Response, ResponseBuilder, ResponseHead};
pub use crate::server::{RequestHandler, Server, ServerOptions};
