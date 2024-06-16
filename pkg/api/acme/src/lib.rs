#![feature(inherent_associated_types)]

#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

mod challenge_solver;
mod client;
mod google_dns_solver;

pub use challenge_solver::*;
pub use client::*;
pub use google_dns_solver::*;

pub const LETSENCRYPT_PROD_DIRECTORY: &'static str =
    "https://acme-v02.api.letsencrypt.org/directory";
pub const LETSENCRYPT_STAGING_DIRECTORY: &'static str =
    "https://acme-staging-v02.api.letsencrypt.org/directory";

pub const DNS_01: &'static str = "dns-01";

/*
Let's Encrypt will issue 90 day certificates.
- Recommendation is to refresh every 60 days.

- If we already have a finished order:
    - If the certificate is valid now and expires in >= N Days, we can use this order
    - TODO: In case orders quickly
- If we have a pending request, must check that the order expiration is

Important things to unit test:
- Failing a challenge closes out a order and we can retry by making a new order
- If we had a recent completed order that already has a certificate, don't re-request one.
- Testing of handling of Retry-After
    - On rate limits or when the order is in the 'pending' state.

*/
