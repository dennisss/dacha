use core::ops::{Deref, DerefMut};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};
use std::{convert::TryFrom, sync::Arc};

use common::errors::*;
use executor::channel::queue::ConcurrentQueue;
use google_auth::*;
use http::uri::Uri;
use http::{AffinityContext, AffinityKey, AffinityKeyCache};

use googleapis_proto::google::longrunning as longrunning_proto;
use googleapis_proto::google::spanner::admin::database::v1 as admin_proto;
use googleapis_proto::google::spanner::v1 as proto;
use parsing::ascii::AsciiString;
use protobuf_builtins::google::protobuf::ListValue;

use crate::database_client::{SpannerDatabaseClientOptions, PRODUCTION_TARGET};

pub struct SpannerDatabaseAdminClient {
    options: SpannerDatabaseClientOptions,

    instance_resource_path: String,

    database_resource_path: String,

    stub: admin_proto::DatabaseAdminStub,
    op_stub: longrunning_proto::OperationsStub,
}

impl SpannerDatabaseAdminClient {
    pub async fn create(options: SpannerDatabaseClientOptions) -> Result<Self> {
        let service_uri: Uri = Uri::try_from(PRODUCTION_TARGET)?;

        let creds = google_auth::GoogleServiceAccountJwtCredentials::create(
            service_uri.clone(),
            options.service_account.clone(),
        )?;

        let mut channel_options =
            rpc::Http2ChannelOptions::try_from(http::ClientOptions::from_uri(&service_uri)?)?;
        channel_options.credentials = Some(Box::new(creds));

        let channel = Arc::new(rpc::Http2Channel::create(channel_options).await?);

        let stub = admin_proto::DatabaseAdminStub::new(channel.clone());
        let op_stub = longrunning_proto::OperationsStub::new(channel);

        let instance_resource_path = format!(
            "projects/{}/instances/{}",
            options.project_id, options.instance_name
        );

        // TODO: Autoderive from the 'google.api.resource' definitions in the API.
        let database_resource_path = format!(
            "projects/{}/instances/{}/databases/{}",
            options.project_id, options.instance_name, options.database_name
        );

        Ok(Self {
            options,
            instance_resource_path,
            database_resource_path,
            stub,
            op_stub,
        })
    }

    pub async fn get_ddl(&self) -> Result<admin_proto::GetDatabaseDdlResponse> {
        let ctx = rpc::ClientRequestContext::default();

        let mut req = admin_proto::GetDatabaseDdlRequest::default();
        req.set_database(&self.database_resource_path);

        self.stub.GetDatabaseDdl(&ctx, &req).await.result
    }

    pub async fn update_ddl(&self, statements: &[String]) -> Result<()> {
        let ctx = rpc::ClientRequestContext::default();

        let mut req = admin_proto::UpdateDatabaseDdlRequest::default();
        req.set_database(&self.database_resource_path);
        for s in statements {
            req.add_statements(s.clone());
        }

        let op = self.stub.UpdateDatabaseDdl(&ctx, &req).await.result?;

        // TODO: Have a hard limit on how long to wait for this?
        self.wait_for_operation(op.name()).await?;

        Ok(())
    }

    pub async fn list_pending_operations(
        &self,
    ) -> Result<admin_proto::ListDatabaseOperationsResponse> {
        let ctx = rpc::ClientRequestContext::default();

        let mut req = admin_proto::ListDatabaseOperationsRequest::default();
        req.set_parent(&self.instance_resource_path);
        // The first part of the filter will only include operations in the selected
        // database (instead of all operations in the instance).
        req.set_filter(format!(
            "(name: \"{}/\") AND (done: false)",
            self.database_resource_path
        ));

        let res = self.stub.ListDatabaseOperations(&ctx, &req).await.result?;
        if !res.next_page_token().is_empty() {
            return Err(err_msg("Paginated operations list not supported"));
        }

        Ok(res)
    }

    pub async fn wait_for_operation(&self, name: &str) -> Result<()> {
        // NOTE: WaitOperation is not implemented on the servers.

        let ctx = rpc::ClientRequestContext::default();

        let mut req = longrunning_proto::GetOperationRequest::default();
        req.set_name(name);

        loop {
            let op = self.op_stub.GetOperation(&ctx, &req).await.result?;
            if !op.done() {
                executor::sleep(Duration::from_secs(5)).await?;
                continue;
            }

            if op.has_error() {
                // TODO: Mark as an external status error (shouldn't be forwarded in an RPC).
                return Err(rpc::Status::from_proto(op.error())?.into());
            }

            break;
        }

        Ok(())
    }
}
