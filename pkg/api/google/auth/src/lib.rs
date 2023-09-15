#[macro_use]
extern crate common;

#[macro_use]
extern crate macros;

mod service_account;

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use common::{bytes::Bytes, errors::*};
use crypto_jwt::{JWTBuilder, JWTPrivateKey};
use executor::sync::Mutex;
use http::uri::Uri;
use macros::Parseable;
use parsing::ascii::AsciiString;

pub use crate::service_account::*;

const GRPC_AUTHORIZATION_METADATA_KEY: &'static str = "authorization";

/// Amount of time for which a single token can be used.
/// NOTE: This exact value is required per the Google AIP-4111 spec.
const TOKEN_LIFETIME: Duration = Duration::from_secs(3600);

/// Minimum time that can be remaining on the token to still be useable.
const REFRESH_THRESHOLD: Duration = Duration::from_secs(240);

/// Attaches self-signed JWT credentials to RPCs (no OAuth2 token exchange).
pub struct GoogleServiceAccountJwtCredentials {
    service_uri: Uri,
    service_account: Arc<GoogleServiceAccount>,
    cached_credential: Mutex<Option<CachedCredential>>,
}

struct CachedCredential {
    authorization_value: AsciiString,
    expiration: SystemTime,
}

impl GoogleServiceAccountJwtCredentials {
    pub fn create(service_uri: Uri, service_account: Arc<GoogleServiceAccount>) -> Result<Self> {
        Ok(Self {
            service_uri,
            service_account,
            cached_credential: Mutex::new(None),
        })
    }

    async fn get_authorization_value(&self) -> Result<AsciiString> {
        let mut cached_cred = self.cached_credential.lock().await;

        let now = SystemTime::now();

        if let Some(cred) = cached_cred.as_ref() {
            let valid = cred.expiration > now
                && cred.expiration.duration_since(now).unwrap() > REFRESH_THRESHOLD;
            if valid {
                return Ok(cred.authorization_value.clone());
            }
        }

        let expiration = now + TOKEN_LIFETIME;

        fn get_seconds(time: SystemTime) -> f64 {
            time.duration_since(UNIX_EPOCH).unwrap().as_secs() as f64
        }

        // This is supposed to be of the form: "https://service.googleapis.com/"
        let audience = {
            let mut uri = self.service_uri.clone();
            uri.path = AsciiString::new("/");
            uri.to_string()?
        };

        let jwt = JWTBuilder::new(JWTPrivateKey::RS256(
            self.service_account.private_key.clone(),
        ))
        .add_header_field("kid", &self.service_account.data.private_key_id)
        .add_claim_string("iss", &self.service_account.data.client_email)
        // TODO: Move this stuff partly to the JWT library.
        .add_claim_string("sub", &self.service_account.data.client_email)
        .add_claim_string("aud", &audience)
        .add_claim_number("iat", get_seconds(now))
        .add_claim_number("exp", get_seconds(expiration))
        .build()?;

        let authorization_value = AsciiString::new(format!("Bearer {}", jwt).as_str());

        *cached_cred = Some(CachedCredential {
            authorization_value: authorization_value.clone(),
            expiration,
        });

        Ok(authorization_value)
    }
}

#[async_trait]
impl rpc::ChannelCredentialsProvider for GoogleServiceAccountJwtCredentials {
    async fn attach_request_credentials(
        &self,
        service_name: &str,
        method_name: &str,
        request_context: &mut rpc::ClientRequestContext,
    ) -> Result<()> {
        // TODO: Ideally precompute the credentials before the channel is marked as
        // ready and do asyncronous refreshing of the tokens.

        let value = self.get_authorization_value().await?;

        request_context
            .metadata
            .add_text(GRPC_AUTHORIZATION_METADATA_KEY, value)?;

        Ok(())
    }
}
