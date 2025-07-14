
/// Header sent in requests to serers that contains the principal string for the end user that initiated the
/// request.
///
/// (e.g. 'dns:user-name.user.zone.cluster.internal').
pub const FORWARDED_ENTITY_HEADER: &'static str = "X-Forwarded-Entity";