#[macro_use]
extern crate macros;

use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use base_error::*;
use base_units::format_duration_secs;
use common::bytes::Bytes;
use executor_multitask::RootResource;
use cluster_client::ClusterMetaClient;
use cluster_client::meta::ObjectMetadataTable;
use cluster_client::ClusterServer;
use container_proto::cluster::ObjectMetadata;
use rpc_util::NamedPortArg;
use db_table::query_one;
use db_table::db::ProtobufDBTransaction;
use google_auth::GoogleServiceAccount;
use public_ip::public_ip;


const POLL_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Args)]
struct Args {
    port: NamedPortArg,
    refresher: RefresherArgs
}

#[derive(Args)]
struct RefresherArgs {
    /// DNS name for which to set the DNS records.
    /// e.g. 'google.com'
    dns_name: String,

    /// Name of the metastore object containing a GCP service account to
    /// use for setting DNS records
    google_service_account_object: String,
}

const SERVICE_ACL_PROTO: &'static str = r#"
    rules: []
"#;

// TODO: Dedup this.
async fn get_object(txn: &ProtobufDBTransaction<'_>, name: &str) -> Result<Option<Vec<u8>>> {
    let obj = query_one!(txn, ObjectMetadataTable, "name = ?", name);
    match obj {
        Some(v) => Ok(Some(v.data().into())),
        None => Ok(None)
    }
}

// NOTE: Retrying on failures will happen with restarting the worker.
async fn run(args: RefresherArgs, client: Arc<ClusterMetaClient>) -> Result<()> {

    let mut txn = client.db().new_transaction().await?;
    let sa_data = String::from_utf8(get_object(&txn, &args.google_service_account_object).await?
        .ok_or_else(|| err_msg("No service account found"))?)?;
    drop(txn);

    let sa: Arc<GoogleServiceAccount> =
        Arc::new(GoogleServiceAccount::parse_json(&sa_data)?);

    let rest_client = Arc::new(google_auth::GoogleRestClient::create(sa.clone())?);
    let dns_client = google_dns::Client::new(sa.project_id(), rest_client)?;

    // TODO: Eventually also set up port forarding from the network's router to the frontend job.

    loop {
        let ips = vec![public_ip().await?];

        let names: Vec<String> = vec![
            format!("{}.", args.dns_name),
            format!("*.{}.", args.dns_name)
        ];

        for name in names {
            let changed = dns_client.set_address_records(&name, 300, &ips).await?;
            if changed {
                println!("Updated A/AAAA for {} to {:?}", name, ips);
            }
        }

        executor::sleep(POLL_INTERVAL).await?;
    }


    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    
    let service = RootResource::new();

    let client = ClusterMetaClient::create_from_environment().await?;
    service.register_dependency(client.clone()).await;

    let mut acl = container_proto::cluster::ServiceACLProto::default();
    protobuf::text::parse_text_proto(SERVICE_ACL_PROTO, &mut acl)?;

    let mut server = ClusterServer::new(args.port.value(), acl, client.clone())?;
    service.register_dependency(server.start()?).await;

    service.spawn_interruptable("Refresher", run(args.refresher, client)).await;

    service.wait().await
}



