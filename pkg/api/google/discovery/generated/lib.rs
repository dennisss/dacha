#[macro_use]
extern crate macros;

use std::sync::Arc;

use common::errors::*;
use google_auth::GoogleRestClient;
use reflection::SerializeTo;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));
