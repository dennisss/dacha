use std::time::SystemTime;

use common::errors::*;
use executor::lock;
use executor::sync::AsyncMutex;
use parsing::ascii::AsciiString;

use crate::constants::*;

#[async_trait]
pub trait AuthorizationRefresher: 'static {
    async fn refresh_authorization_value(&self) -> Result<Credential>;
}

pub struct Credential {
    pub authorization_value: AsciiString,
    pub expiration: SystemTime,
}

pub struct AuthorizationProvider<R> {
    refresher: R,
    cached_credential: AsyncMutex<Option<Credential>>,
}

impl<R: AuthorizationRefresher> AuthorizationProvider<R> {
    pub fn new(refresher: R) -> Self {
        Self {
            refresher,
            cached_credential: AsyncMutex::new(None),
        }
    }

    pub async fn get_authorization_value(&self) -> Result<AsciiString> {
        let mut cached_cred = self.cached_credential.lock().await?.read_exclusive();

        let now = SystemTime::now();

        if let Some(cred) = cached_cred.as_ref() {
            let valid = cred.expiration > now
                && cred.expiration.duration_since(now).unwrap() > REFRESH_THRESHOLD;
            if valid {
                return Ok(cred.authorization_value.clone());
            }
        }

        let cred = self.refresher.refresh_authorization_value().await?;
        let ret = cred.authorization_value.clone();

        lock!(c <= cached_cred.upgrade(), {
            *c = Some(cred);
        });

        Ok(ret)
    }
}
