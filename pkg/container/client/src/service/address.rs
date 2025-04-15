use crate::id::{entity_id_from_string, entity_id_to_string, is_valid_entity_id};
use crate::service::zone::*;

const NAME_SUFFIX: &'static str = ".cluster.internal";

/// A 'url' / address pointing to a server in the cluster that can be sent
/// requests.
///
/// Note that ServiceAddresses in string form may have 'local' in them
///
/// Note that unlike a ServiceName, a ServiceAddress can use the 'local' zone to
/// reference the
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceAddress {
    pub name: ServiceName,

    /// NOTE: Only valid for Job and Worker entities.
    pub port: Option<String>,
}

/// Absolute global identifier for an entity in a cluster.
///
/// Note that ServiceNames always reference a specific zone and don't use the
/// 'local' zone.
///
/// TODO: REname to 'EntityName'
#[derive(Clone, Hash, Debug, PartialEq, Eq)]
pub struct ServiceName {
    zone: String,

    entity: ServiceEntity,
}

// TODO: Prevent a user from constructing this without going through one of the parsing helpers.
#[derive(Clone, Hash, Debug, PartialEq, Eq)]
pub enum ServiceEntity {
    Node { id: u64 },
    Job { job_name: String },
    Worker { job_name: String, worker_id: String },
    User { name: String },
    Root,
}

#[derive(Debug, Fail)]
pub enum ServiceParseError {
    NotClusterAddress,
    NameTooShort,
    InvalidNodeId,
    UnknownEntity,
    InvalidZone,
    InvalidEntityName,
}

impl std::fmt::Display for ServiceParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(self, f)
    }
}

impl ServiceAddress {
    pub fn is_service_address(address: &str) -> bool {
        let host_end = address.rfind(':').unwrap_or(address.len());
        address[..host_end].ends_with(NAME_SUFFIX)
    }

    /// Parses a relative address that path relatively reference an entity in
    /// the same zone as the caller.
    pub fn parse_relative_addr(
        address: &str,
        current_zone: &str,
    ) -> Result<Self, ServiceParseError> {
        if !is_valid_zone(current_zone) {
            return Err(ServiceParseError::InvalidZone);
        }

        let raw_name = address
            .strip_suffix(NAME_SUFFIX)
            .ok_or(ServiceParseError::NotClusterAddress)?;

        let mut name_parts = raw_name.split(".").collect::<Vec<_>>();

        let mut port = None;
        if let Some(first_part) = name_parts.get(0) {
            if let Some(port_name) = first_part.strip_prefix("_") {
                port = Some(port_name.to_string());
                name_parts.remove(0);
            }
        }

        let name = ServiceName::parse_impl(name_parts, Some(current_zone))?;

        Ok(ServiceAddress { name, port })
    }
}

impl ServiceName {
    pub fn for_job(zone: &str, job_name: &str) -> Result<Self, ServiceParseError> {
        if !is_valid_zone(zone) {
            return Err(ServiceParseError::InvalidZone);
        }

        Ok(Self {
            zone: zone.to_string(),
            entity: ServiceEntity::Job {
                job_name: job_name.to_string(),
            },
        })
    }

    pub fn for_worker(zone: &str, worker_name: &str) -> Result<Self, ServiceParseError> {
        if !is_valid_zone(zone) {
            return Err(ServiceParseError::InvalidZone);
        }

        let (job_name, worker_id) = worker_name
            .rsplit_once(".")
            .ok_or(ServiceParseError::NameTooShort)?;

        Ok(Self {
            zone: zone.to_string(),
            entity: ServiceEntity::Worker {
                worker_id: worker_id.to_string(),
                job_name: job_name.to_string(),
            },
        })
    }

    pub fn for_node(zone: &str, node_id: u64) -> Result<Self, ServiceParseError> {
        if !is_valid_zone(zone) {
            return Err(ServiceParseError::InvalidZone);
        }

        if !is_valid_entity_id(node_id) {
            return Err(ServiceParseError::InvalidNodeId);
        }

        Ok(Self {
            zone: zone.to_string(),
            entity: ServiceEntity::Node { id: node_id },
        })
    }

    pub fn for_user(zone: &str, name: &str) -> Result<Self, ServiceParseError> {
        if !is_valid_zone(zone) {
            return Err(ServiceParseError::InvalidZone);
        }

        if !is_valid_user_name(name) {
            return Err(ServiceParseError::InvalidEntityName);
        }

        Ok(Self {
            zone: zone.to_string(),
            entity: ServiceEntity::User { name: name.to_string() },
        })
    }

    pub fn for_root(zone: &str) -> Result<Self, ServiceParseError> {
        if !is_valid_zone(zone) {
            return Err(ServiceParseError::InvalidZone);
        }

        Ok(Self {
            zone: zone.to_string(),
            entity: ServiceEntity::Root,
        })
    }

    pub fn parse(name: &str) -> Result<Self, ServiceParseError> {
        Self::parse_relative(name, None)
    }

    pub fn parse_relative(
        name: &str,
        current_zone: Option<&str>
    ) -> Result<Self, ServiceParseError> {
        let raw_name = name
            .strip_suffix(NAME_SUFFIX)
            .ok_or(ServiceParseError::NotClusterAddress)?;

        let mut name_parts = raw_name.split(".").collect::<Vec<_>>();

        Self::parse_impl(name_parts, current_zone)
    }

    fn parse_impl(
        mut name_parts: Vec<&str>,
        current_zone: Option<&str>,
    ) -> Result<Self, ServiceParseError> {
        // Name must contain at least a zone, an entity type.
        if name_parts.len() < 2 {
            return Err(ServiceParseError::NameTooShort);
        }

        let mut zone = name_parts.pop().unwrap();

        if zone == LOCAL_ZONE {
            match current_zone {
                Some(z) => zone = z,
                None => {
                    return Err(ServiceParseError::InvalidZone);
                }
            }
        } else if zone == GLOBAL_ZONE {
            return Err(ServiceParseError::InvalidZone);
        }

        if !is_valid_zone(zone) {
            return Err(ServiceParseError::InvalidZone);
        }

        let entity_type = name_parts.pop().unwrap();

        // TODO: Also validate job name patterns?
        let entity = match entity_type {
            "node" => {
                if name_parts.len() != 1 {
                    return Err(ServiceParseError::InvalidNodeId);
                }

                let id =
                    entity_id_from_string(name_parts[0]).ok_or(ServiceParseError::InvalidNodeId)?;

                ServiceEntity::Node { id }
            }
            "job" => {
                let job_name = name_parts.into_iter().rev().collect::<Vec<_>>().join(".");
                ServiceEntity::Job { job_name }
            }
            "worker" => {
                // Must at least have a job name and worker index.
                if name_parts.len() < 2 {
                    return Err(ServiceParseError::NameTooShort);
                }

                let worker_id = name_parts[0].to_string();

                let job_name = (&name_parts[1..])
                    .iter()
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(".");

                ServiceEntity::Worker {
                    job_name,
                    worker_id,
                }
            }
            "user" => {
                if name_parts.len() != 1 {
                    return Err(ServiceParseError::NameTooShort);
                }
                
                let name = name_parts[0];
                if !is_valid_user_name(name) {
                    return Err(ServiceParseError::InvalidEntityName);
                }

                ServiceEntity::User { name: name.to_string() }
            }
            "root" => {
                if name_parts.len() != 0 {
                    return Err(ServiceParseError::NameTooShort);
                }

                ServiceEntity::Root
            }
            _ => {
                return Err(ServiceParseError::UnknownEntity);
            }
        };

        Ok(ServiceName {
            zone: zone.to_string(),
            entity,
        })
    }

    pub fn zone(&self) -> &str {
        &self.zone
    }

    pub fn entity(&self) -> &ServiceEntity {
        &self.entity
    }

    pub fn to_string(&self) -> String {
        match &self.entity {
            ServiceEntity::Node { id } => {
                format!(
                    "{}.node.{}{}",
                    entity_id_to_string(*id).unwrap(),
                    self.zone,
                    NAME_SUFFIX
                )
            }
            ServiceEntity::Job { job_name } => {
                format!(
                    "{}.job.{}{}",
                    job_name
                        .split('.')
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("."),
                    self.zone,
                    NAME_SUFFIX
                )
            }
            ServiceEntity::Worker {
                job_name,
                worker_id,
            } => {
                format!(
                    "{}.{}.worker.{}{}",
                    worker_id,
                    job_name
                        .split('.')
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("."),
                    self.zone,
                    NAME_SUFFIX
                )
            }
            ServiceEntity::User { name } => {
                format!("{}.user.{}{}", name, self.zone, NAME_SUFFIX)
            }
            ServiceEntity::Root => {
                format!("root.{}{}", self.zone, NAME_SUFFIX)
            }
        }
    }

    /// Returns whether or not it is possible for us to attempt to make a remote
    /// connection to this entity.
    pub fn maybe_reachable(&self) -> bool {
        match &self.entity {
            ServiceEntity::Node { .. }
            | ServiceEntity::Job { .. }
            | ServiceEntity::Worker { .. } => true,
            ServiceEntity::Root | ServiceEntity::User { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use common::errors::*;

    use super::*;

    #[test]
    fn parse_job_address_with_port() -> Result<()> {
        let addr = ServiceAddress::parse_relative_addr(
            "_my_port.adder_server.user.job.local.cluster.internal",
            "testing",
        )?;
        assert_eq!(
            addr,
            ServiceAddress {
                port: Some("my_port".into()),
                name: ServiceName {
                    zone: "testing".into(),
                    entity: ServiceEntity::Job {
                        job_name: "user.adder_server".into()
                    }
                }
            }
        );

        assert_eq!(
            addr.name.to_string(),
            "adder_server.user.job.testing.cluster.internal"
        );

        Ok(())
    }

    #[test]
    fn parse_root_name() -> Result<()> {
        let addr = ServiceName::parse("root.home.cluster.internal")?;

        assert_eq!(addr.entity(), &ServiceEntity::Root);
        assert_eq!(addr.zone(), "home");

        Ok(())
    }

    #[test]
    fn parse_user_name() -> Result<()> {
        let addr = ServiceName::parse("dennis.user.home.cluster.internal")?;

        assert_eq!(addr.entity(), &ServiceEntity::User { name: "dennis".into() });
        assert_eq!(addr.zone(), "home");

        assert_eq!(addr.to_string(), "dennis.user.home.cluster.internal");

        Ok(())
    }

    #[test]
    fn parse_worker_address_with_port() -> Result<()> {
        let addr = ServiceAddress::parse_relative_addr(
            "a12345.adder_client.user.worker.local.cluster.internal",
            "testing",
        )?;
        assert_eq!(
            addr,
            ServiceAddress {
                port: None,
                name: ServiceName {
                    zone: "testing".into(),
                    entity: ServiceEntity::Worker {
                        job_name: "user.adder_client".into(),
                        worker_id: "a12345".into()
                    }
                }
            }
        );

        assert_eq!(
            addr.name.to_string(),
            "a12345.adder_client.user.worker.testing.cluster.internal"
        );

        Ok(())
    }
}
