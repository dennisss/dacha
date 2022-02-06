const NAME_SUFFIX: &'static str = ".cluster.internal";

pub struct ServiceAddress {
    pub name: ServiceName,

    /// NOTE: Only valid for Job and Task entities.
    pub port: Option<String>,
}

pub struct ServiceName {
    pub zone: String,
    pub entity: ServiceEntity,
}

pub enum ServiceEntity {
    Node { id: u64 },
    Job { job_name: String },
    Task { job_name: String, task_id: String },
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

        let mut port = None;

        // TODO: Also validate job name patterns?
        let entity = match entity_type {
            "node" => {
                if name_parts.len() != 1 {
                    return Err(ServiceParseError::InvalidNodeId);
                }

                let id = u64::from_str_radix(name_parts[0], 16)
                    .map_err(|_| ServiceParseError::InvalidNodeId)?;
                ServiceEntity::Node { id }
            }
            "job" => {
                let job_name = name_parts.into_iter().rev().collect::<Vec<_>>().join(".");
                ServiceEntity::Job { job_name }
            }
            "task" => {
                // Must at least have a job name and task index.
                if name_parts.len() < 2 {
                    return Err(ServiceParseError::NameTooShort);
                }

                let task_id = name_parts[0].to_string();

                let job_name = (&name_parts[1..])
                    .iter()
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(".");

                ServiceEntity::Task { job_name, task_id }
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
    pub fn for_task(zone: &str, task_name: &str) -> Result<Self, ServiceParseError> {
        let (task_id, job_name) = task_name
            .split_once(".")
            .ok_or(ServiceParseError::NameTooShort)?;

        Ok(Self {
            zone: zone.to_string(),
            entity: ServiceEntity::Task {
                task_id: task_id.to_string(),
                job_name: job_name.to_string(),
            },
        })
    }

    pub fn to_string(&self) -> String {
        match &self.entity {
            ServiceEntity::Node { id } => {
                format!("{:08x}.node.{}{}", *id, self.zone, NAME_SUFFIX)
            }
            ServiceEntity::Job { job_name } => {
                format!(
                    "{}.job{}{}",
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
            ServiceEntity::Task { job_name, task_id } => {
                format!(
                    "{}.{}.task{}{}",
                    task_id,
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
