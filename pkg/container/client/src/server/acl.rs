use std::collections::HashMap;

use std::sync::Arc;

use common::errors::*;
use common::hash::FastHasherBuilder;
use container_proto::cluster::*;
use db_table::db::ProtobufDB;
use executor_multitask::ServiceResource;
use http::ServerHandler;
use rpc_util::AddReflection;

use crate::acl::checker::*;
use crate::server::router::PathRouter;
use crate::service::address::ServiceName;
use crate::{acl::principal::*, meta::client::ClusterMetaClient};

pub struct ServiceACL {
    proto: ServiceACLProto,
    zone: String,
    db: Option<Arc<ProtobufDB>>,
    router: PathRouter<Rule>,
}

struct Rule {
    proto: ServiceACLProto_Rule,
    principals: Vec<Principal>,
}

// TODO: Check that all regular routes are covered by some rule in the
// ServiceACL (and that ACLs don't cover non-existend routes)

impl ServiceACL {
    pub fn create(proto: ServiceACLProto, zone: &str, db: Option<Arc<ProtobufDB>>) -> Result<Self> {
        let mut router = PathRouter::default();
        for proto in proto.rules() {
            let mut principals = vec![];
            if proto.principals().is_empty() {
                return Err(err_msg("Empty principals list in rule"));
            }

            for s in proto.principals() {
                principals.push(Principal::parse(s)?);
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

        Ok(Self {
            proto,
            zone: zone.to_string(),
            db,
            router,
        })
    }

    pub fn allow_unauthenticated(&self) -> bool {
        self.proto.allow_unauthenticated()
    }

    /// Determines if the given entity is allowed to issue the given request.
    /// (only checking high level that the path/method is allowed).
    pub async fn is_allowed(
        &self,
        entity: Option<&ServiceName>,
        request: &http::Request,
    ) -> Result<bool> {
        let path = request.head.uri.path.as_str();

        let mut allowed_principals = PrincipalSet::default();

        if let Some((_, rule)) = self.router.route(path) {
            // TODO: Check http method.

            allowed_principals.extend(rule.principals.iter().cloned());
        }

        check_entity_allowed(
            entity,
            &allowed_principals,
            &self.zone,
            self.db.as_ref().map(|s| s.as_ref()),
        )
        .await
    }
}
