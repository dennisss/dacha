use std::collections::HashMap;

use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use cluster_proto::cluster::*;
use db_table::db::ProtobufDB;
use executor_multitask::ServiceResource;
use http::ServerHandler;
use rpc_util::AddReflection;

use crate::acl::checker::*;
use crate::server::router::PathRouter;
use crate::service::address::ServiceName;
use crate::{acl::principal::*, meta::client::ClusterMetaClient};
use crate::acl::proxy::FORWARDED_ENTITY_HEADER;

pub struct ServiceACL {
    proto: ServiceACLProto,
    zone: String,
    db: Option<Arc<ProtobufDB>>,
    router: PathRouter<Rule>,
    trusted_proxies: PrincipalSet,
    delegatable_principals: PrincipalSet,
}

pub enum EffectiveEntity {
    Resolved(Option<ServiceName>, bool),
    
    /// The peer tried to delegate a role but we don't allow them to.
    Denied,
}

struct Rule {
    proto: ServiceACLProto_Rule,
    principals: PrincipalSet,
}

// TODO: Check that all regular routes are covered by some rule in the
// ServiceACL (and that ACLs don't cover non-existend routes)

impl ServiceACL {
    pub fn create(proto: ServiceACLProto, zone: &str, db: Option<Arc<ProtobufDB>>) -> Result<Self> {
        let mut router = PathRouter::default();
        for proto in proto.rules() {
            let mut principals = PrincipalSet::default();
            if proto.principals().is_empty() {
                return Err(err_msg("Empty principals list in rule"));
            }

            for s in proto.principals() {
                principals.insert(Principal::parse_relative(s, Some(zone))?);
            }

            router.add_route(
                proto.path(),
                proto.is_directory(),
                Rule {
                    proto: proto.as_ref().clone(),
                    principals,
                },
            )?;
        }

        let mut trusted_proxies = PrincipalSet::default();
        for s in proto.trusted_proxies() {
            trusted_proxies.insert(Principal::parse_relative(s, Some(zone))?);
        }

        let mut delegatable_principals = PrincipalSet::default();
        for s in proto.delegatable_principals() {
            delegatable_principals.insert(Principal::parse_relative(s, Some(zone))?);
        }

        Ok(Self {
            proto,
            zone: zone.to_string(),
            db,
            router,
            trusted_proxies,
            delegatable_principals,
        })
    }

    pub fn allow_unauthenticated_connections(&self) -> bool {
        self.proto.allow_unauthenticated_connections()
    }

    pub async fn resolve_effective_entity(
        &self,
        peer_entity: Option<&ServiceName>,
        request: &http::Request,
    ) -> Result<EffectiveEntity> {
        let header = match request.head.headers.get_one(FORWARDED_ENTITY_HEADER) {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(EffectiveEntity::Resolved(peer_entity.cloned(), false)),
            Err(e) => {
                // Multiple delegation headers.
                return Ok(EffectiveEntity::Denied);
            }
        };

        let peer_is_proxy = check_entity_allowed(
            peer_entity,
            &self.trusted_proxies,
            &self.zone,
            self.db.as_ref().map(|s| s.as_ref()),
        )
        .await?;

        if !peer_is_proxy {
            eprintln!("Rejecting entity delegation. Peer not a trusted proxy: {:?}", peer_entity);
            return Ok(EffectiveEntity::Denied);
        }

        let entity = match Self::parse_forwarded_entity(&header) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Received invalid entity header: {}", e);
                return Ok(EffectiveEntity::Denied);
            }
        };

        let delegatable = check_entity_allowed(
            entity.as_ref(),
            &self.delegatable_principals,
            &self.zone,
            self.db.as_ref().map(|s| s.as_ref()),
        )
        .await?;

        if !delegatable {
            eprintln!("Rejecting entity delegation. Not allowed to delegate to: {:?}", entity);
            return Ok(EffectiveEntity::Denied);
        }

        Ok(EffectiveEntity::Resolved(entity, true))
    }

    fn parse_forwarded_entity(header: &http::Header) -> Result<Option<ServiceName>> {
        let principal_string = header.value.to_utf8_str()?;
        let principal = Principal::parse(&principal_string)?;
        let entity = principal.to_entity()?;
        Ok(entity)
    }

    /// Determines if the given entity is allowed to issue the given request.
    /// (only checking high level that the path/method is allowed).
    pub async fn is_allowed(
        &self,
        entity: Option<&ServiceName>,
        request: &http::Request,
    ) -> Result<bool> {
        let path = request.head.uri.path.as_str();

        let mut allowed_principals = &PrincipalSet::default();

        if let Some((_, rule)) = self.router.route(path) {
            // TODO: Check http method.

            allowed_principals = &rule.principals;
        }

        check_entity_allowed(
            entity,
            allowed_principals,
            &self.zone,
            self.db.as_ref().map(|s| s.as_ref()),
        )
        .await
    }
}
