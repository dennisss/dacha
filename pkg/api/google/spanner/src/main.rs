extern crate common;
#[macro_use]
extern crate macros;
extern crate executor;

use std::{convert::TryFrom, sync::Arc};

use common::errors::*;
use google_auth::*;
use http::uri::Uri;

use googleapis_proto::google::spanner::admin::database::v1 as admin;
use googleapis_proto::google::spanner::v1 as spanner;
use protobuf_builtins::google::protobuf::{ListValue, ValueProto};

/*
Define a DatabaseTable
*/

#[executor_main]
async fn main() -> Result<()> {
    // https://spanner.googleapis.com

    /*
    TODO: When connecting to something like pubsub.googleapis.com, only create one connection.

    Querying stuff:
    - Read( keys )

    How to store a protobuf in spanner:
    -

    MessageOptions

    Can I have a const value for a real proto?
        - Hard because if there are nested messages, we need a MessagePtr


    User::descriptor().options()

    - In order to load the options, I need to be able to interpret the message type as a dynamic message (or dynamic type) and assign a value to it
    - An extension() is basically an unknown field

    - extensions are weird as they can be any type.

    */

    let data =
        file::read_to_string("/home/dennis/.credentials/dacha-main-748d2acba112.json").await?;

    let sa: Arc<GoogleServiceAccount> =
        Arc::new(google_auth::GoogleServiceAccount::parse_json(&data)?);

    /*
    let client = google_spanner::SpannerDatabaseClient::create(
        google_spanner::SpannerDatabaseClientOptions {
            project_id: "dacha-main".to_string(),
            instance_name: "instance-1".to_string(),
            database_name: "study".to_string(),
            service_account: sa,
        },
    )
    .await?;
    */

    // Intent based

    let client = google_spanner::SpannerDatabaseAdminClient::create(
        google_spanner::SpannerDatabaseClientOptions {
            project_id: "dacha-main".to_string(),
            instance_name: "instance-1".to_string(),
            database_name: "study".to_string(),
            service_account: sa,
            session_count: 2,
        },
    )
    .await?;

    // Op ids will look like:
    // "projects/dacha-main/instances/instance-1/databases/study/operations/
    // _auto_op_a8b3651fa382e884"

    println!("{:?}", client.list_pending_operations().await?);

    client.wait_for_operation("projects/dacha-main/instances/instance-1/databases/study/operations/_auto_op_a8b3651fa382e884").await?;

    return Ok(());

    let res = client.get_ddl().await?;

    for statement in res.statements() {
        println!("{}", statement);

        println!(
            "{:#?}",
            google_spanner::sql::DdlStatement::parse(statement)?
        );
    }

    // println!("{:?}", client.get_ddl().await?);

    /*

    */

    /*
    let mut values = ListValue::default();
    values.new_values().set_string_value("123");
    values.new_values().set_string_value("Dennis");
    values.new_values().set_string_value("Shtatnov");
    values.new_values().set_string_value("densht@gmail.com");

    client
        .insert(
            "User",
            &["Id", "FirstName", "LastName", "EmailAddress"],
            &[values],
        )
        .await?;
    */

    // let context = rpc::ClientRequestContext::default();

    /*
    let stub = googleapis_proto::google::storage::v2::StorageStub::new(channel);

    let mut req = googleapis_proto::google::storage::v2::ListBucketsRequest::default();

    req.set_parent("dacha-main");

    let mut res = stub.ListBuckets(&context, &req).await;
    */

    // let stub = admin::DatabaseAdminStub::new(channel.clone());

    /*


    let mut req =
        googleapis_proto::google::spanner::admin::database::v1::ListDatabasesRequest::default();

    req.set_parent("projects/dacha-main/instances/instance-1");

    let mut res = stub.ListDatabases(&context, &req).await;
    */

    // //

    // // UpdateDatabaseDdl

    // println!("{:?}", res.result);

    Ok(())
}

/*
General plan:

- dacha.dev is the main domain
    - Has a Cookie for '.dacha.dev' containing the main credential
-


  options.apiEndpoint ||
        options.servicePath ||
        v1.SpannerClient.servicePath,

google-cloud-resource-prefix'
x-goog-spanner-route-to-leader


*/
