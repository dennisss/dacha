use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::bytes::Bytes;
use common::errors::*;
use crypto_jwt::*;
use parsing::ascii::AsciiString;
use reflection::ParseFrom;

use crate::constants::*;
use crate::provider::*;
use crate::service_account::GoogleServiceAccount;

const OAUTH2_TOKEN_URI: &'static str = "https://oauth2.googleapis.com/token";

pub struct GoogleServiceAccountOAuth2Credentials {
    inner: AuthorizationProvider<Impl>,
}

struct Impl {
    // TODO: If we need to connect to many APIs, it would be useful to re-use this client across
    // them all.
    client: http::SimpleClient,
    service_account: Arc<GoogleServiceAccount>,
    scope: String,
}

#[derive(Debug, Parseable)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: usize,
}

impl GoogleServiceAccountOAuth2Credentials {
    pub fn create<S: AsRef<str>>(
        service_account: Arc<GoogleServiceAccount>,
        scopes: &[S],
    ) -> Result<Self> {
        // TODO: We can configure this client to be very lazy with connections as
        // refreshes should be in-frequent.
        let client = http::SimpleClient::new(http::SimpleClientOptions::default());

        let scope = scopes
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>()
            .join(" ");

        Ok(Self {
            inner: AuthorizationProvider::new(Impl {
                client,
                service_account,
                scope,
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

        let jwt_expiration = now + JWT_TOKEN_LIFETIME;

        fn get_seconds(time: SystemTime) -> f64 {
            time.duration_since(UNIX_EPOCH).unwrap().as_secs() as f64
        }

        // NOTE: The lifetime of the JWT doesn't seem to impact the lifetime of the
        // token returned by the API.

        let jwt = JWTBuilder::new(JWTPrivateKey::RS256(
            self.service_account.private_key.clone(),
        ))
        .add_header_field("kid", &self.service_account.data.private_key_id)
        .add_claim_string("iss", &self.service_account.data.client_email)
        .add_claim_string("scope", &self.scope)
        // TODO: Move this stuff partly to the JWT library.
        .add_claim_string("aud", OAUTH2_TOKEN_URI)
        .add_claim_number("iat", get_seconds(now))
        .add_claim_number("exp", get_seconds(jwt_expiration))
        .build()?;

        let req = http::RequestBuilder::new()
            .method(http::Method::POST)
            .uri(OAUTH2_TOKEN_URI)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .build()?;

        // TODO: Replace with a proper serializer.
        let body = Bytes::from(format!(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={}",
            jwt
        ));

        let res = self
            .client
            .request(&req.head, body, &http::ClientRequestContext::default())
            .await?;

        if res.head.status_code != http::status_code::OK {
            return Err(format_err!(
                "OAuth2 token refresh failed [HTTP {:?}] {:?}",
                res.head.status_code,
                res.body
            ));
        }

        let value = json::parse(std::str::from_utf8(&res.body)?)?;

        let object = TokenResponse::parse_from(json::ValueParser::new(&value))?;

        let expiration = now + Duration::from_secs(object.expires_in as u64);

        let authorization_value = AsciiString::new(&format!("Bearer {}", object.access_token));

        Ok(Credential {
            authorization_value,
            expiration,
        })
    }
}
