#![feature(core_intrinsics, trait_alias)]

#[macro_use]
extern crate common;
extern crate parsing;
#[macro_use]
extern crate failure;

#[macro_use]
extern crate regexp_macros;

#[macro_use]
extern crate arrayref;

extern crate crypto;

mod proto;
mod body;
mod chunked;
mod chunked_syntax;
mod common_syntax;
mod dns;
pub mod header;
mod header_syntax;
pub mod message;
mod message_syntax;
mod reader;
mod message_body;
pub mod server;
mod spec;
pub mod status_code;
pub mod encoding;
mod encoding_syntax;
pub mod uri;
pub mod uri_syntax;
mod method;
mod request;
mod response;
mod v2;
mod headers;
pub mod static_file_handler;
pub mod query;
mod hpack;
mod client;

// Public exports.
pub use crate::server::{Server, RequestHandler};
pub use crate::client::{Client, ClientOptions};
pub use crate::request::{Request, RequestBuilder, RequestHead};
pub use crate::response::{Response, ResponseBuilder, ResponseHead};
pub use crate::body::{Body, BodyFromData, EmptyBody, WithTrailers};
pub use crate::method::Method;
pub use crate::header::{Headers, Header};