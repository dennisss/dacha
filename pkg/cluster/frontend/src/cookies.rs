

/// Cookie which contains a unique random id to identify a specific client.
///
/// Generated to be a url safe base64 encoded 64-bit random number.
pub const CLIENT_ID_COOKIE: &'static str = "Client-Id";

/// Secret base64 encoded bytes that are used to authenticate against a user session.
pub const AUTH_KEY_COOKIE: &'static str = "Auth-Key";
