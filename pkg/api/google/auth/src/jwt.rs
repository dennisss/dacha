use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use common::{bytes::Bytes, errors::*};
use crypto_jwt::{JWTBuilder, JWTPrivateKey};
use http::uri::Uri;
use macros::Parseable;
use parsing::ascii::AsciiString;

use crate::constants::*;
use crate::provider::*;
use crate::service_account::GoogleServiceAccount;

const GRPC_AUTHORIZATION_METADATA_KEY: &'static str = "authorization";

/// Attaches self-signed JWT credentials to RPCs (no OAuth2 token exchange).
pub struct GoogleServiceAccountJwtCredentials {
    inner: AuthorizationProvider<Impl>,
}

struct Impl {
    service_uri: Uri,
    service_account: Arc<GoogleServiceAccount>,
}

impl GoogleServiceAccountJwtCredentials {
    pub fn create(service_uri: Uri, service_account: Arc<GoogleServiceAccount>) -> Result<Self> {
        Ok(Self {
            inner: AuthorizationProvider::new(Impl {
                service_uri,
                service_account,
            }),
        })
    }

    pub async fn get_authorization_value(&self) -> Result<AsciiString> {
        self.inner.get_authorization_value().await
    }
}

#[async_trait]
impl AuthorizationRefresher for Impl {
    async fn refresh_authorization_value(&self) -> Result<Credential> {
        let now = SystemTime::now();
        let expiration = now + JWT_TOKEN_LIFETIME;

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

        Ok(Credential {
            authorization_value: authorization_value.clone(),
            expiration,
        })
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
