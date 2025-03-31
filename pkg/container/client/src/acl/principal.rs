use std::collections::HashSet;

use common::{errors::*, hash::FastHasherBuilder};

use crate::service::address::{ServiceEntity, ServiceName};

pub type PrincipalSet = HashSet<Principal, FastHasherBuilder>;

/// A principal is a single entity or group of entities that can be granted
/// access to some resources.
///
/// The leaf most entities are users without any identity 'Unauthenticated' and
/// those which have a validated DNS name ('ServiceName')
///
/// Usually this will be encoded as a string. e.g.
/// - 'unauthenticated'
///   - Refers to a user with no known identity.
/// - 'authenticated'
///   - Refers to any user with a known identity (signed by our cluster level
///     CA).
/// - 'dns:something.job.zone.cluster.internal'
///   - Terminal entity identified by a DNS name (usually authenticated via TLS
///     client credentials).
/// - 'pattern:**.job.*.cluster.internal'
///   - Any worker of any job recognized by the local cluster.
///   - '*' can be used to match a wildcard DNS segment (not containing a '.').
///   - '**' can be used to match any number of arbitrary DNS segments.
/// - 'zone:zone_name:group:group_name'
///   - A group of entities. The group definition exists in a single cluster's
///     metastore (located via the given zone name).
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum Principal {
    /// Matches every entity (whether authenticated or not).
    Unauthenticated,

    /// Matches every entity with a valid identity.
    ///
    /// AVOID USING THIS WHERE POSSIBLE
    Authenticated,

    /// NOTE: Worker entities are always normalized to job entities.
    Entity(ServiceName),

    /// Matches one or more DNS names.
    Pattern(String),

    Group {
        zone: String,
        name: String,
    },
}

impl Principal {
    pub fn parse(value: &str) -> Result<Self> {
        if value == "unauthenticated" {
            return Ok(Self::Unauthenticated);
        }

        if value == "authenticated" {
            return Ok(Self::Authenticated);
        }

        if let Some(name) = value.strip_prefix("dns:") {
            // TODO: Do worker normalization.
            let name = ServiceName::parse(name)?;
            return Ok(Self::Entity(name));
        }

        if let Some(pattern) = value.strip_prefix("pattern:") {
            return Ok(Self::Pattern(pattern.to_string()));
        }

        let parts = value.split(':').collect::<Vec<_>>();
        if parts.len() == 4 && parts[0] == "zone" && parts[2] == "group" {
            return Ok(Self::Group {
                zone: parts[1].to_string(),
                name: parts[3].to_string(),
            });
        }

        Err(format_err!("Invalid principal string: {}", value))
    }

    pub fn to_string(&self) -> String {
        // TODO: Must validate the zone and group name allows for reparsing.

        match self {
            Principal::Unauthenticated => "unauthenticated".into(),
            Principal::Authenticated => "authenticated".into(),
            Principal::Entity(name) => format!("dns:{}", name.to_string()),
            Principal::Pattern(v) => format!("pattern:{}", v),
            Principal::Group { zone, name } => format!("zone:{}:group:{}", zone, name),
        }
    }
}
