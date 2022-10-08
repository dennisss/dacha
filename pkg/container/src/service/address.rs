const NAME_SUFFIX: &'static str = ".cluster.internal";

#[derive(Debug, PartialEq)]
pub struct ServiceAddress {
    pub name: ServiceName,

    /// NOTE: Only valid for Job and Worker entities.
    pub port: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct ServiceName {
    pub zone: String,
    pub entity: ServiceEntity,
}

#[derive(Debug, PartialEq)]
pub enum ServiceEntity {
    Node { id: u64 },
    Job { job_name: String },
    Worker { job_name: String, worker_id: String },
}

#[derive(Debug, Fail)]
pub enum ServiceParseError {
    NotClusterAddress,
    NameTooShort,
    InvalidNodeId,
    UnknownEntity,
}

impl std::fmt::Display for ServiceParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        std::fmt::Debug::fmt(self, f)
    }
}

impl ServiceAddress {
    pub fn is_service_address(address: &str) -> bool {
        address.ends_with(NAME_SUFFIX)
    }

    pub fn parse(address: &str, current_zone: &str) -> Result<Self, ServiceParseError> {
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

        // Name must contain at least a zone, an entity type and an entity name.
        if name_parts.len() < 3 {
            return Err(ServiceParseError::NameTooShort);
        }

        let mut zone = name_parts.pop().unwrap();
        if zone == "local" {
            zone = current_zone;
        }

        let entity_type = name_parts.pop().unwrap();

        // TODO: Also validate job name patterns?
        let entity = match entity_type {
            "node" => {
                if name_parts.len() != 1 {
                    return Err(ServiceParseError::InvalidNodeId);
                }

                let id = common::base32::base32_decode_cl64(name_parts[0])
                    .ok_or(ServiceParseError::InvalidNodeId)?;
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
            _ => {
                return Err(ServiceParseError::UnknownEntity);
            }
        };

        Ok(ServiceAddress {
            name: ServiceName {
                zone: zone.to_string(),
                entity,
            },
            port,
        })
    }
}

impl ServiceName {
    pub fn for_worker(zone: &str, worker_name: &str) -> Result<Self, ServiceParseError> {
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

    pub fn to_string(&self) -> String {
        match &self.entity {
            ServiceEntity::Node { id } => {
                format!(
                    "{}.node.{}{}",
                    common::base32::base32_encode_cl64(*id),
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
        }
    }
}

#[cfg(test)]
mod tests {
    use common::errors::*;

    use super::*;

    // TODO: Why is 'async' needed here.
    #[test]
    async fn parse_job_address_with_port() -> Result<()> {
        let addr = ServiceAddress::parse(
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
    async fn parse_worker_address_with_port() -> Result<()> {
        let addr = ServiceAddress::parse(
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
