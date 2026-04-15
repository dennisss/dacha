use std::time::Duration;

use crate::service::address::{ServiceEntity, ServiceName};

const ROOT_CERT_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 365 * 4); // 4 years.

/// Amount of time after which we insert a new root certificate into the
/// metastore until every client will have the certificate in its local
/// registry.
///
/// (The root certificate can't be used to sign any child certificates until it
/// is fully propagated. Else some clients may not recognize it as a valid
/// parent certificate)
pub const ROOT_CERT_PROPAGATION_DELAY: Duration = Duration::from_secs(60 * 60 * 24 * 30); // 1 month.

const NODE_CERT_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 180); // 0.5 years

/// NOTE: If the CA or metastore nodes go down, then they must down come back
/// online within this amount of time to avoid the cluter needing to be
/// re-bootstrapped.
///
/// TODO: Lower this temporarily during node testing.
const WORKER_CERT_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 31); // 1 month

/// Minimum amount of time remaining on a worker certificate in order to trying
/// to immediately starting the worker (if it isn't already started).
pub const WORKER_CERT_MIN_REMAINING: Duration = Duration::from_secs(60 * 60 * 2); // 2 hours

const USER_CERT_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 31); // 1 month

/// Duration used for certificates that just live on developer/user machines to enable
/// connection to servers not running in a cluster (on 'localhost'). 
pub const LOCALHOST_CERT_DURATION: Duration = Duration::from_secs(60 * 60 * 24 * 365 * 4); // 4 years

/// Gets the default certificate lifetime for specific types of entites.
///
/// Note that this may change over time so for existing certificates, the x509
/// metadata should be used as the source of truth.
///
/// Returns None for entites that shouldn't get certificates.
pub fn cert_duration_for_entity(entity: &ServiceEntity) -> Option<Duration> {
    Some(match entity {
        ServiceEntity::Node { .. } => NODE_CERT_DURATION,
        ServiceEntity::Worker { .. } => WORKER_CERT_DURATION,
        ServiceEntity::Root => ROOT_CERT_DURATION,
        ServiceEntity::User { .. } => USER_CERT_DURATION,
        ServiceEntity::Job { .. } => {
            return None;
        }
    })
}

/// For a specific type of entity, if its TLS certificate has <= this amount of
/// time remaining before expiration, then it should request a refresh.
pub fn cert_refresh_below_duration(entity: &ServiceEntity) -> Option<Duration> {
    cert_duration_for_entity(entity).map(|v| v / 2)
}
