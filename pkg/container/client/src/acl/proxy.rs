
/// Header sent in requests to servers that contains the principal string for the end user that initiated the
/// request.
///
/// (e.g. 'dns:user-name.user.zone.cluster.internal').
///
/// NOTE: This will always be present on requests to the frontend to backends.
pub const FORWARDED_ENTITY_HEADER: &'static str = "X-Forwarded-Entity";

/// NOTE: This has the same name and value format as the standard header with this name.
pub const FORWARDED_IP_HEADER: &'static str = "X-Forwarded-For";

/// Header sent in requests to servers from the frontend job to indicate that the
/// request originates from an end user logged in to the session with this
/// base64url encoded id. 
pub const SESSION_ID_HEADER: &'static str = "X-Session-Id";

/// Header sent in requests to servers from the frontend job to indicate that the
/// request originates from an end user device which has this client id (in
/// base64url encoded form) associated with it.
///
/// NOTE: This will always be present on requests from the frontend to backends.
pub const CLIENT_ID_HEADER: &'static str = "X-Client-Id";
