use std::time::Duration;

/// Amount of time for which a single token can be used.
/// NOTE: This exact value is required per the Google AIP-4111 spec.
pub const JWT_TOKEN_LIFETIME: Duration = Duration::from_secs(3600);

/// Minimum time that can be remaining on the token to still be useable.
pub const REFRESH_THRESHOLD: Duration = Duration::from_secs(240);
