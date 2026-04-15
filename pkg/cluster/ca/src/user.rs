// Utilities for handling user password based authentication.
// TODO: Maybe move this into a separate crate.

use std::sync::Arc;
use std::time::Duration;
use std::iter::FromIterator;

use base_error::*;
use cluster_client::{ClusterServerConnectionData, ClusterServerRequestData};
use cluster_client::meta::UserTable;
use cluster_client::acl::checker::check_entity_allowed;
use cluster_client::service::address::ServiceName;
use cluster_client::acl::principal::PrincipalSet;
use cluster_client::acl::principal::Principal;
use cluster_client::ClusterMetaClient;
use common::bytes::Bytes;
use cluster_proto::cluster::*;
use crypto::tls::FileCredentialsManager;
use crypto::x509::Certificate;
use crypto::bcrypt::*;
use db_table::{query, query_one};
use db_table::db::ProtobufDBTransaction;


/// Looks up a user with the given name and verifies they have the given password.
pub async fn get_user_with_password(
    user_name: &str,
    user_password: &str,
    txn: &ProtobufDBTransaction<'_>,
) -> Result<User> {
    let user = query_one!(txn, UserTable, "name = ?", user_name)
        .ok_or_else(|| rpc::Status::not_found("No such user"))?;

    if !bcrypt_verify(user.password_digest(), user_password) {
        return Err(rpc::Status::permission_denied("Incorrect password").into());
    }

    Ok(user)
}

pub async fn change_user_password(
    request: &ChangePasswordRequest,
    require_current_password: bool,
    requester: Option<&ServiceName>,
    client: &ClusterMetaClient
) -> Result<()> {

    let target_entity = ServiceName::for_user(client.zone(), request.user_name())
        .map_err(|_| rpc::Status::invalid_argument("Invalid user name"))?;

    // Even if the user knows the current password, we still require verifying that
    // they are logged in to ensure that any 2FA checks have been cleared.
    let allowed_to_change_pass =
        check_entity_allowed(
            requester,
            &PrincipalSet::from_iter([Principal::Entity(target_entity)].iter().cloned()),
            client.zone(),
            Some(client.db())
        ).await?;

    if !allowed_to_change_pass {
        return Err(rpc::Status::failed_precondition(
            "Not allowed to change this users password",
        )
        .into());
    }

    let mut txn = client.db().new_transaction().await?;

    // Current password check not required since we verify that the peer directly has a valid certificate.
    change_user_password_raw(request, require_current_password, &mut txn).await?;

    txn.commit().await?;

    Ok(())
}


/// NOTE: This assumes that the user is authorized to change the password.
async fn change_user_password_raw(
    request: &ChangePasswordRequest,
    require_current_password: bool,
    txn: &mut ProtobufDBTransaction<'_>
) -> Result<()> {

    let mut user = query_one!(txn, UserTable, "name = ?", request.user_name())
        .ok_or_else(|| rpc::Status::not_found("No such user"))?;

    if request.current_password().is_empty() {
        if require_current_password {
            return Err(rpc::Status::invalid_argument("Must specify the current password").into());
        }
    } else {
        if !bcrypt_verify(user.password_digest(), request.current_password()) {
            return Err(rpc::Status::permission_denied("Incorrect current password").into());
        }
    }

    let new_digest = bcrypt_encode(request.new_password())
        .map_err(|_| rpc::Status::invalid_argument("Invalid new password"))?;

    user.set_password_digest(new_digest);

    txn.put::<UserTable>(&user).await?;

    Ok(())
}

async fn hash_password(password: &str) -> Result<String> {
    Ok(bcrypt_encode(password)
        .map_err(|_| rpc::Status::invalid_argument("Invalid new password"))?)
}