/*
Notes:
- Dates are number of seconds since epoch
- Everything in the JOSE header must be well understood when doing validation
- No whitespace is allowed in the base64 url encoded form.
- Values in the JOSE header are case insensitive
    - TODO: Should I also treat keys as case insensitive?

TODOs:
- Internally implement the following claim fields
    "exp"
    "nbf" 0 Not before
- Add a unit test for the insecure form:
    - https://datatracker.ietf.org/doc/html/rfc7519#section-6.1



*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate asn;

extern crate crypto;
extern crate json;

mod algorithm;
mod key;
mod signature;

pub use algorithm::*;
pub use key::*;
pub use signature::*;

use std::collections::HashMap;
use std::time::SystemTime;

use common::errors::*;
use crypto::rsa::RSAPrivateKey;

const TYP: &'static str = "typ";
const ALG: &'static str = "alg";
const JWT: &'static str = "JWT";

/*
/// Disassembled/parsed JWT.
pub struct JWTParts {
    /// JOSE header
    header: json::Value,

    claims_set: json::Value,
    claims_set_payload: Vec<u8>,
    signature: Vec<u8>,
}
*/

pub enum JWTPrivateKey {
    None,
    RS256(RSAPrivateKey),
}

pub struct JWTBuilder {
    private_key: JWTPrivateKey,
    header: json::Value,
    claims_set: json::Value,
}

impl JWTBuilder {
    pub fn new(private_key: JWTPrivateKey) -> Self {
        let mut header = json::Value::Object(HashMap::new());
        header.set_field(TYP, JWT);

        let algorithm = match &private_key {
            JWTPrivateKey::None => JWTAlgorithm::None,
            JWTPrivateKey::RS256(_) => JWTAlgorithm::RS256,
        };

        header.set_field(ALG, algorithm.to_value());

        let mut claims_set: json::Value = json::Value::Object(HashMap::new());

        Self {
            private_key,
            header,
            claims_set,
        }
    }

    pub fn add_header_field(mut self, name: &str, value: &str) -> Self {
        self.header.set_field(name, value);
        self
    }

    pub fn add_claim_string(mut self, name: &str, value: &str) -> Self {
        self.claims_set.set_field(name, value);
        self
    }

    pub fn add_claim_number(mut self, name: &str, value: f64) -> Self {
        self.claims_set.set_field(name, json::Value::Number(value));
        self
    }

    pub fn build(self) -> Result<String> {
        let header = json::stringify(&self.header)?;
        let claims_set = json::stringify(&self.claims_set)?;

        let plaintext = format!(
            "{}.{}",
            base_radix::base64url_encode(header.as_bytes()),
            base_radix::base64url_encode(claims_set.as_bytes())
        );

        let mut signature = vec![];
        match &self.private_key {
            JWTPrivateKey::None => {}
            JWTPrivateKey::RS256(key) => {
                let signer = crypto::rsa::RSASSA_PKCS_v1_5::sha256();
                signature = signer.create_signature(key, plaintext.as_bytes())?;
            }
        }

        Ok(format!(
            "{}.{}",
            plaintext,
            base_radix::base64url_encode(&signature),
        ))
    }
}
