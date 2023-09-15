#![feature(core_intrinsics, trait_alias)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate common;
extern crate parsing;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate regexp_macros;
extern crate crypto;
extern crate net;
extern crate sys;

mod alpn;
mod body;
mod chunked;
mod chunked_syntax;
mod client;
mod common_syntax;
mod connection_event_listener;
pub mod cors;
mod dns;
pub mod encoding;
mod encoding_syntax;
pub mod header;
mod header_syntax;
pub mod headers; // TODO: Make this private?
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
mod response_channel;
pub mod server;
mod server_handler;
mod spec;
pub mod static_file_handler;
pub mod status_code;
pub mod uri;
pub mod uri_syntax;
mod v1;
pub mod v2;

// Public exports.
pub use crate::body::{Body, BodyFromData, BodyFromParts, EmptyBody, WithTrailers};
pub use crate::client::{
    AffinityContext, AffinityKey, AffinityKeyCache, Client, ClientInterface, ClientOptions,
    ClientRequestContext, ResolvedEndpoint, Resolver, ResolverChangeListener, SimpleClient,
    SimpleClientOptions, SystemDNSResolver,
};
pub use crate::header::{Header, Headers};
pub use crate::method::Method;
pub use crate::request::{Request, RequestBuilder, RequestHead};
pub use crate::response::{BufferedResponse, Response, ResponseBuilder, ResponseHead};
pub use crate::server::{BoundServer, Server, ServerOptions};
pub use crate::server_handler::*;
