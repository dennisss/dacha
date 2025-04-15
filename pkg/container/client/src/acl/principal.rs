use std::collections::HashSet;

use common::{errors::*, hash::FastHasherBuilder};

use crate::service::address::{ServiceEntity, ServiceName};
use crate::service::zone::LOCAL_ZONE;

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
/// - 'group:[zone_name/]:group_name'
///   - A group of entities. The group definition exists in a single cluster's
///     metastore (located via the given zone name or the local zone if none specified).
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum Principal {
    /// Matches no entities.
    Nobody,

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
        Self::parse_relative(value, None)
    }

    pub fn parse_relative(value: &str, current_zone: Option<&str>) -> Result<Self> {
        if value == "nobody" {
            return Ok(Self::Nobody);
        }

        if value == "unauthenticated" {
            return Ok(Self::Unauthenticated);
        }

        if value == "authenticated" {
            return Ok(Self::Authenticated);
        }

        if let Some(name) = value.strip_prefix("dns:") {
            let mut name = ServiceName::parse_relative(name, current_zone)?;

            if let ServiceEntity::Worker {
                job_name,
                worker_id,
            } = name.entity()
            {
                let normalized = ServiceName::for_job(name.zone(), &job_name)?;
                name = normalized;
            }

            return Ok(Self::Entity(name));
        }

        if let Some(pattern) = value.strip_prefix("pattern:") {
            if let Some(prefix) = pattern.strip_suffix(".local.cluster.internal") {
                let zone = current_zone.ok_or_else(|| err_msg("Not parsing relative pattern"))?;
                return Ok(Self::Pattern(format!("{}.{}.cluster.internal", prefix, zone)));
            }

            return Ok(Self::Pattern(pattern.to_string()));
        }

        if let Some(name) = value.strip_prefix("group:") {
            let (zone, group) = match name.split_once("/") {
                Some((mut zone, group)) => {
                    if zone == LOCAL_ZONE {
                        zone = current_zone.ok_or_else(|| err_msg("Not parsing relative group"))?;
                    }

                    (zone, group)
                },
                None => {
                    let zone = current_zone.ok_or_else(|| err_msg("Not parsing relative group"))?;
                    (zone, name)
                }
            };

            return Ok(Self::Group {
                zone: zone.to_string(),
                name: group.to_string(),
            });
        }

        Err(format_err!("Invalid principal string: {}", value))
    }

    pub fn to_string(&self) -> String {
        // TODO: Must validate the zone and group name allows for reparsing.

        match self {
            Principal::Nobody => "nobody".into(),
            Principal::Unauthenticated => "unauthenticated".into(),
            Principal::Authenticated => "authenticated".into(),
            Principal::Entity(name) => format!("dns:{}", name.to_string()),
            Principal::Pattern(v) => format!("pattern:{}", v),
            Principal::Group { zone, name } => format!("group:{}/{}", zone, name),
        }
    }
}
