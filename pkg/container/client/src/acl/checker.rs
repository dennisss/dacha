use common::errors::*;
use db_table::db::{ProtobufDB, ProtobufDBTransaction};
use db_table::{query, query_one};

use crate::acl::principal::*;
use crate::meta::GroupMembershipTable;
use crate::service::address::{ServiceEntity, ServiceName};

pub struct ACLChecker<'a> {
    snapshot: Option<Box<ProtobufDBTransaction<'a>>>,
}

/*
TODO: Cache:
- 'entity_name' in 'group' ranked based on EMA hit rate with a max TTL on information.

*/

/// Determines if 'entity' matches any of the principals in 'allowlist'.
///
/// This may require recursively checking for group memberships usign 'db'
pub async fn check_entity_allowed(
    entity: Option<&ServiceName>,
    allowlist: &PrincipalSet,
    current_zone: &str,
    db: Option<&ProtobufDB>,
) -> Result<bool> {
    let mut entity = entity.cloned();

    if let Some(name) = &mut entity {
        // A root in the current zone has access to all resources in that zone.
        if name.zone() == current_zone {
            if let ServiceEntity::Root = name.entity() {
                return Ok(true);
            }
        }

        // Normalize workers to jobs.
        if let ServiceEntity::Worker {
            job_name,
            worker_id,
        } = name.entity()
        {
            let normalized = ServiceName::for_job(name.zone(), &job_name)?;
            *name = normalized;
        }
    }

    let entity_dns_name = match &entity {
        Some(v) => Some(v.to_string()),
        None => None,
    };

    // TODO: Need to dedup / cache stuff.
    // TODO: Need to prevent group cycles.
    let mut pending = allowlist.iter().cloned().collect::<Vec<Principal>>();

    // TODO: Check all cheap to evaluate principals before groups.
    while let Some(allowed_principal) = pending.pop() {
        match &allowed_principal {
            Principal::Unauthenticated => return Ok(true),
            Principal::Authenticated => {
                if entity.is_some() {
                    return Ok(true);
                }
            }
            Principal::Entity(allowed_name) => {
                if let Some(name) = &entity {
                    if name == allowed_name {
                        return Ok(true);
                    }
                }
            }
            Principal::Pattern(pattern) => {
                let dns_name = match &entity_dns_name {
                    Some(v) => v,
                    None => continue,
                };

                if match_dns_pattern(&dns_name, &pattern)? {
                    return Ok(true);
                }
            }
            Principal::Group { zone, name } => {
                if zone != current_zone {
                    return Err(err_msg(
                        "Checking group memberships across zones is not supported",
                    ));
                }

                let db = db.ok_or_else(|| {
                    err_msg("Group matching not supported without a DB connection")
                })?;

                if let Some(entity_name) = &entity {
                    let entity_principal = format!("dns:{}", entity_name.to_string());

                    let direct_membership = query_one!(
                        db,
                        GroupMembershipTable,
                        "group_name = ? AND expands = FALSE AND member = ?",
                        name,
                        entity_principal
                    );

                    if direct_membership.is_some() {
                        return Ok(true);
                    }
                }

                let indirect_memberships = query!(
                    db,
                    GroupMembershipTable,
                    "group_name = ? AND expands = TRUE",
                    name
                );

                for membership in indirect_memberships {
                    pending.push(Principal::parse(membership.member())?);
                }
            }
        }
    }

    Ok(false)
}

fn match_dns_pattern(dns_name: &str, pattern: &str) -> Result<bool> {
    let mut pattern_iter = pattern.rsplit('.');
    let mut name_iter = dns_name.rsplit('.');

    let mut matched = true;
    while let Some(pattern_part) = pattern_iter.next() {
        let name_part = match name_iter.next() {
            Some(v) => v,
            None => return Ok(false),
        };

        if pattern_part == "*" {
            continue;
        } else if pattern_part == "**" {
            if pattern_iter.next().is_some() {
                return Err(err_msg(
                    "In a name pattern, '**' must only be used in the first segment",
                ));
            }

            return Ok(true);
        }

        if name_part != pattern_part {
            matched = false;
            break;
        }
    }

    // Besides matching all parts of the pattern, no additional prefix segments must
    // be present in the name.
    if name_iter.next().is_some() {
        return Ok(false);
    }

    if matched {
        return Ok(true);
    }

    Ok(matched)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn match_pattern_test() {
        let test_cases = &[
            ("example.com", "google.com", Some(false)),
            ("example.com", "example.com", Some(true)),
            ("test.example.com", "example.com", Some(false)),
            ("test.example.com", "*.example.com", Some(true)),
            ("a.test.example.com", "*.example.com", Some(false)),
            ("a.test.example.com", "a.*.example.com", Some(true)),
            ("b.test.example.com", "a.*.example.com", Some(false)),
            ("b.test.example.com", "**.example.com", Some(true)),
            ("example.com", "**.example.com", Some(false)),
            ("b.test.example.com", "b.**.example.com", None),
        ];

        for (name, pat, res) in test_cases {
            assert_eq!(match_dns_pattern(*name, *pat).ok(), *res);
        }
    }
}
