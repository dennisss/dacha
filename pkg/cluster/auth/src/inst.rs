use std::sync::Arc;
use std::time::{Duration, SystemTime};

use base_error::*;
use cluster_client::credentials::{cert_duration_for_entity};
use cluster_client::{ClusterServerConnectionData, ClusterServerRequestData};
use cluster_client::meta::{UserTable};
use cluster_client::{
    meta::{client::ClusterMetaClient},
    service::address::{ServiceEntity, ServiceName},
};
use cluster_client::acl::checker::check_entity_allowed;
use cluster_client::throttler::HashedTokenBucketThrottler;
use common::bytes::Bytes;
use container_proto::cluster::*;
use db_table::{query, query_one};
use file::LocalPath;
use cluster_client::acl::proxy::{SESSION_ID_HEADER, CLIENT_ID_HEADER};
use cluster_client::meta::SessionTable;
use cluster_ca::user::{get_user_with_password, change_user_password};

use crate::utils::*;

pub struct ClusterAuthImpl {
    meta_client: Arc<ClusterMetaClient>,
    throttler: HashedTokenBucketThrottler
}

impl ClusterAuthImpl {

    pub async fn new(meta_client: Arc<ClusterMetaClient>) -> Self {
        // Roughly 1 QPS per client.
        let throttler = HashedTokenBucketThrottler::create(
            128,
            10,
            Duration::from_secs(10)
        ).await;

        Self {
            meta_client,
            throttler,
        }
    }


    async fn session_info_impl(
        &self,
        request: &SessionInfoRequest,
        context: &rpc::ServerRequestContext,
    ) -> Result<SessionInfoResponse> {
        // TODO: Rate limit this (can grab the user name from the effective_user)

        let cluster_context = ClusterServerRequestData::from_rpc_context(context)?;

        // When sitting behind a trusted proxy, we can assume user authentication has already.
        // Else, we can't trust the value of the session id header.
        //
        // TODO: When testing without a proxy resolve it based on the Auth-Key header.
        // (it would be easiest to put that logic in the )
        if !cluster_context.peer_is_trusted_proxy {
            return Err(rpc::Status::failed_precondition("Must be running behing a proxy").into());
        }
        
        let mut response = SessionInfoResponse::default();

        let session_id = Self::parse_session_id_header(&context.metadata)
            .map_err(|_| rpc::Status::invalid_argument("Request contains invalid session id"))?;

        if let Some(session_id) = session_id {
            let txn = self.meta_client.db().new_transaction().await?;
            let session = query_one!(txn, SessionTable, "id = ?", session_id)
                .ok_or_else(|| rpc::Status::not_found("No such session"))?;

            let user = query_one!(txn, UserTable, "name = ?", session.user_name())
                .ok_or_else(|| rpc::Status::not_found("No such user"))?;
            
            response.session_mut();
            response.user_mut().set_name(user.name());

            // TODO: Check that the effective user matches the session?
        }

        Ok(response)
    }

    /// NOTE: That assumes that the header can be trusted.
    fn parse_session_id_header(metadata: &rpc::Metadata) -> Result<Option<u64>> {
        Self::parse_u64_header(metadata, SESSION_ID_HEADER)
    }

    fn parse_client_id_header(metadata: &rpc::Metadata) -> Result<Option<ClientId>> {
        Ok(Self::parse_u64_header(metadata, CLIENT_ID_HEADER)?.map(|v| ClientId(v)))
    }

    fn parse_u64_header(metadata: &rpc::Metadata, name: &str) -> Result<Option<u64>> {
        let text = match metadata.get_text(name)? {
            Some(v) => v,
            None => return Ok(None)
        };

        let data = base_radix::base64url_decode(&text)?;
        if data.len() != 8 {
            return Err(err_msg("Wrong id length"));
        }

        Ok(Some(u64::from_be_bytes(*array_ref![data, 0, 8])))
    }

    async fn login_impl(
        &self,
        request: &SessionLoginRequest,
        context: &rpc::ServerRequestContext,
        response: &mut rpc::ServerResponse<'_, SessionLoginResponse>,
    ) -> Result<()> {
        if !self.throttler.take(request.user_name().as_bytes(), 1) {
            return Err(rpc::Status::resource_exhausted("Exceeded per-user request limit.").into());
        }

        let cluster_context = ClusterServerRequestData::from_rpc_context(context)?;

        // Required for trusting the request headers.
        if !cluster_context.peer_is_trusted_proxy {
            return Err(rpc::Status::failed_precondition("Must be running behing a proxy").into());
        }

        let client_id = Self::parse_client_id_header(&context.metadata)
            .map_err(|_| rpc::Status::invalid_argument("Invalid client id"))?
            .ok_or_else(|| rpc::Status::invalid_argument("Missing client id"))?;

        let session_id = Self::parse_session_id_header(&context.metadata)
            .map_err(|_| rpc::Status::invalid_argument("Request contains invalid session id"))?;
        if session_id.is_some() {
            return Err(rpc::Status::invalid_argument("Already logged in").into());
        }

        // TODO: Check that we aren't already logged in (no session id and no effective entity)
        
        let mut txn = self.meta_client.db().new_transaction().await?;

        let user = get_user_with_password(request.user_name(), request.user_password(), &txn).await?;

        let auth_key = generate_session_auth_key().await;

        let mut session = Session::default();
        session.set_id(generate_session_id().await);
        session.set_user_name(user.name());
        session.set_auth_key_hash(create_session_auth_key_hash(&auth_key));
        session.set_client_id(client_id.0);
        session.set_created_at(SystemTime::now());

        // TODO: Session metadata (ideally pull raw HTTP headers without gRPC parsing).
        // - User-Agent
        // - X-Forwarded-For

        txn.put::<SessionTable>(&session).await?;
        txn.commit().await?;

        response.context.metadata.head_metadata.add_text(
            AUTH_KEY_HEADER, &base_radix::base64url_encode(&auth_key)
        )?;

        Ok(())
    }

    async fn change_password_impl(
        &self,
        request: &ChangePasswordRequest,
        context: &rpc::ServerRequestContext,
    ) -> Result<ChangePasswordResponse> {
        if !self.throttler.take(request.user_name().as_bytes(), 1) {
            return Err(rpc::Status::resource_exhausted("Exceeded per-user request limit.").into());
        }

        let cluster_context = ClusterServerRequestData::from_rpc_context(context)?;

        // Explicitly requiring password check since the other has no other login credentials.
        change_user_password(
            request,
            true,
            cluster_context.effective_entity.as_ref(),
            &self.meta_client
        ).await?;

        Ok(ChangePasswordResponse::default())
    }

    async fn logout_impl(
        &self,
        request: &LogoutRequest,
        context: &rpc::ServerRequestContext,
        response: &mut rpc::ServerResponse<'_, LogoutResponse>,
    ) -> Result<()> {
        let cluster_context = ClusterServerRequestData::from_rpc_context(context)?;

        // Required to trust the request's session id header.
        if !cluster_context.peer_is_trusted_proxy {
            return Err(rpc::Status::failed_precondition("Must be running behing a proxy").into());
        }

        let session_id = Self::parse_session_id_header(&context.metadata)
            .map_err(|_| rpc::Status::invalid_argument("Request contains invalid session id"))?
            .ok_or_else(|| rpc::Status::invalid_argument("Request not logged in"))?;

        let mut txn = self.meta_client.db().new_transaction().await?;
        
        let mut session = query_one!(txn, SessionTable, "id = ?", session_id)
            .ok_or_else(|| rpc::Status::not_found("No such session"))?;
        
        if !session.deleted() {
            session.set_deleted(true);
            txn.put::<SessionTable>(&session).await?;
            txn.commit().await?;
        }

        // NOTE: Extra cookie options will be added by the frontend.
        response.context.metadata.head_metadata.add_text(
            AUTH_KEY_HEADER, AUTH_KEY_DELETED_VALUE
        )?;

        Ok(())
    }
}

#[async_trait]
impl UserSessionAuthenticationService for ClusterAuthImpl {
    async fn SessionInfo(
        &self,
        request: rpc::ServerRequest<SessionInfoRequest>,
        response: &mut rpc::ServerResponse<SessionInfoResponse>,
    ) -> Result<()> {
        response.value = self
            .session_info_impl(&request.value, &request.context)
            .await?;
        Ok(())
    }

    async fn Login(
        &self,
        request: rpc::ServerRequest<SessionLoginRequest>,
        response: &mut rpc::ServerResponse<SessionLoginResponse>,
    ) -> Result<()> {
        self.login_impl(&request.value, &request.context, response).await?;
        Ok(())
    }

    async fn ChangePassword(
        &self,
        request: rpc::ServerRequest<ChangePasswordRequest>,
        response: &mut rpc::ServerResponse<ChangePasswordResponse>,
    ) -> Result<()> {
        response.value = self
            .change_password_impl(&request.value, &request.context)
            .await?;
        Ok(())
    }

    async fn Logout(
        &self,
        request: rpc::ServerRequest<LogoutRequest>,
        response: &mut rpc::ServerResponse<LogoutResponse>,
    ) -> Result<()> {
        self.logout_impl(&request.value, &request.context, response).await?;
        Ok(())
    }
}
